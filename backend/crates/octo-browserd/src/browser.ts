import {
  chromium,
  firefox,
  webkit,
  devices,
  type Browser,
  type BrowserContext,
  type Page,
  type Frame,
  type Dialog,
  type Request,
  type Route,
  type Locator,
  type CDPSession,
} from "playwright";
import path from "node:path";
import os from "node:os";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import {
  type RefMap,
  type EnhancedSnapshot,
  type SnapshotOptions,
  getEnhancedSnapshot,
  parseRef,
} from "./snapshot.js";
import { defaultUserAgent, defaultHeaders } from "./defaults.js";

export interface ScreencastFrame {
  data: string;
  metadata: {
    offsetTop: number;
    pageScaleFactor: number;
    deviceWidth: number;
    deviceHeight: number;
    scrollOffsetX: number;
    scrollOffsetY: number;
    timestamp?: number;
  };
  sessionId: number;
}

export interface ScreencastOptions {
  format: "jpeg" | "png";
  quality: number;
  maxWidth: number;
  maxHeight: number;
  everyNthFrame: number;
}

export interface LaunchOptions {
  headless?: boolean;
  viewport?: { width: number; height: number };
  browser?: "chromium" | "firefox" | "webkit";
  headers?: Record<string, string>;
  executablePath?: string;
  extensions?: string[];
  profile?: string;
  storageState?: string;
  proxy?: { server: string; bypass?: string; username?: string; password?: string };
  args?: string[];
  userAgent?: string;
  ignoreHTTPSErrors?: boolean;
  allowFileAccess?: boolean;
}

interface TrackedRequest {
  url: string;
  method: string;
  headers: Record<string, string>;
  timestamp: number;
  resourceType: string;
}

interface ConsoleMessage {
  type: string;
  text: string;
  timestamp: number;
}

interface PageError {
  message: string;
  timestamp: number;
}

export class BrowserManager {
  private browser: Browser | null = null;
  private isPersistentContext = false;
  private contexts: BrowserContext[] = [];
  private pages: Page[] = [];
  private activePageIndex = 0;
  private activeFrame: Frame | null = null;
  private dialogHandler: ((dialog: Dialog) => Promise<void>) | null = null;
  private trackedRequests: TrackedRequest[] = [];
  private routes: Map<string, (route: Route) => Promise<void>> = new Map();
  private consoleMessages: ConsoleMessage[] = [];
  private pageErrors: PageError[] = [];
  private isRecordingHar = false;
  private refMap: RefMap = {};
  private lastSnapshot = "";
  private scopedHeaderRoutes: Map<string, (route: Route) => Promise<void>> = new Map();

  private cdpSession: CDPSession | null = null;
  private screencastActive = false;
  private frameCallback: ((frame: ScreencastFrame) => void) | null = null;
  private screencastFrameHandler: ((params: unknown) => void) | null = null;

  private recordingContext: BrowserContext | null = null;
  private recordingPage: Page | null = null;
  private recordingOutputPath = "";
  private recordingTempDir = "";

  // --- State queries ---

  isLaunched(): boolean {
    return this.browser !== null || this.isPersistentContext;
  }

  getPage(): Page {
    if (this.pages.length === 0) {
      throw new Error("Browser not launched. Call launch first.");
    }
    return this.pages[this.activePageIndex];
  }

  getFrame(): Frame {
    if (this.activeFrame) return this.activeFrame;
    return this.getPage().mainFrame();
  }

  getViewportSize(): { width: number; height: number } | null {
    if (this.pages.length === 0) return null;
    return this.pages[this.activePageIndex].viewportSize();
  }

  getActiveIndex(): number {
    return this.activePageIndex;
  }

  isScreencasting(): boolean {
    return this.screencastActive;
  }

  isRecording(): boolean {
    return this.recordingContext !== null;
  }

  // --- Snapshot / refs ---

  async getSnapshot(options?: SnapshotOptions): Promise<EnhancedSnapshot> {
    const page = this.getPage();
    const snapshot = await getEnhancedSnapshot(page, options);
    this.refMap = snapshot.refs;
    this.lastSnapshot = snapshot.tree;
    return snapshot;
  }

  getRefMap(): RefMap {
    return this.refMap;
  }

  getLocatorFromRef(refArg: string): Locator | null {
    const ref = parseRef(refArg);
    if (!ref) return null;

    const refData = this.refMap[ref];
    if (!refData) return null;

    const page = this.getPage();

    if (refData.role === "clickable" || refData.role === "focusable") {
      return page.locator(refData.selector);
    }

    let locator: Locator;
    if (refData.name) {
      locator = page.getByRole(refData.role as Parameters<Page["getByRole"]>[0], {
        name: refData.name,
        exact: true,
      });
    } else {
      locator = page.getByRole(refData.role as Parameters<Page["getByRole"]>[0]);
    }

    if (refData.nth !== undefined) {
      locator = locator.nth(refData.nth);
    }

    return locator;
  }

  isRef(selector: string): boolean {
    return parseRef(selector) !== null;
  }

  getLocator(selectorOrRef: string): Locator {
    const locator = this.getLocatorFromRef(selectorOrRef);
    if (locator) return locator;
    return this.getPage().locator(selectorOrRef);
  }

  // --- Launch / close ---

  async launch(options: LaunchOptions): Promise<void> {
    if (this.isLaunched()) return;

    const hasExtensions = !!options.extensions?.length;
    const hasProfile = !!options.profile;
    const hasStorageState = !!options.storageState;

    const browserType = options.browser ?? "chromium";
    if (hasExtensions && browserType !== "chromium") {
      throw new Error("Extensions are only supported in Chromium");
    }
    if (options.allowFileAccess && browserType !== "chromium") {
      throw new Error("allowFileAccess is only supported in Chromium");
    }
    if (hasStorageState && hasProfile) {
      throw new Error("Storage state cannot be used with profile");
    }
    if (hasStorageState && hasExtensions) {
      throw new Error("Storage state cannot be used with extensions");
    }

    const launcher =
      browserType === "firefox" ? firefox : browserType === "webkit" ? webkit : chromium;
    const viewport = options.viewport ?? { width: 1280, height: 720 };

    // Apply realistic defaults when none provided (Chromium only -- Firefox
    // and WebKit have their own UA strings that are fine).
    const userAgent =
      options.userAgent ?? (browserType === "chromium" ? defaultUserAgent() : undefined);
    const headers =
      options.headers ?? (browserType === "chromium" ? defaultHeaders() : undefined);

    const fileAccessArgs = options.allowFileAccess
      ? ["--allow-file-access-from-files", "--allow-file-access"]
      : [];
    const baseArgs = options.args
      ? [...fileAccessArgs, ...options.args]
      : fileAccessArgs.length > 0
        ? fileAccessArgs
        : undefined;

    let context: BrowserContext;

    if (hasExtensions) {
      const extPaths = options.extensions!.join(",");
      const session = process.env.AGENT_BROWSER_SESSION || "default";
      const extArgs = [`--disable-extensions-except=${extPaths}`, `--load-extension=${extPaths}`];
      const allArgs = baseArgs ? [...extArgs, ...baseArgs] : extArgs;
      context = await launcher.launchPersistentContext(
        path.join(os.tmpdir(), `octo-browserd-ext-${session}`),
        {
          headless: false,
          executablePath: options.executablePath,
          args: allArgs,
          viewport,
          extraHTTPHeaders: headers,
          userAgent,
          ...(options.proxy && { proxy: options.proxy }),
          ignoreHTTPSErrors: options.ignoreHTTPSErrors ?? false,
        },
      );
      this.isPersistentContext = true;
    } else if (hasProfile) {
      const profilePath = options.profile!.replace(/^~\//, os.homedir() + "/");
      context = await launcher.launchPersistentContext(profilePath, {
        headless: options.headless ?? true,
        executablePath: options.executablePath,
        args: baseArgs,
        viewport,
        extraHTTPHeaders: headers,
        userAgent,
        ...(options.proxy && { proxy: options.proxy }),
        ignoreHTTPSErrors: options.ignoreHTTPSErrors ?? false,
      });
      this.isPersistentContext = true;
    } else {
      this.browser = await launcher.launch({
        headless: options.headless ?? true,
        executablePath: options.executablePath,
        args: baseArgs,
      });
      context = await this.browser.newContext({
        viewport,
        extraHTTPHeaders: headers,
        userAgent,
        ...(options.proxy && { proxy: options.proxy }),
        ignoreHTTPSErrors: options.ignoreHTTPSErrors ?? false,
        ...(options.storageState && { storageState: options.storageState }),
      });
    }

    context.setDefaultTimeout(60000);
    this.contexts.push(context);
    this.setupContextTracking(context);

    const page = context.pages()[0] ?? (await context.newPage());
    if (!this.pages.includes(page)) {
      this.pages.push(page);
      this.setupPageTracking(page);
    }
    this.activePageIndex = this.pages.length > 0 ? this.pages.length - 1 : 0;
  }

  async close(): Promise<void> {
    if (this.recordingContext) {
      await this.stopRecording();
    }

    if (this.screencastActive) {
      await this.stopScreencast();
    }

    if (this.cdpSession) {
      await this.cdpSession.detach().catch(() => {});
      this.cdpSession = null;
    }

    for (const page of this.pages) {
      await page.close().catch(() => {});
    }
    for (const context of this.contexts) {
      await context.close().catch(() => {});
    }
    if (this.browser) {
      await this.browser.close().catch(() => {});
      this.browser = null;
    }

    this.pages = [];
    this.contexts = [];
    this.isPersistentContext = false;
    this.activePageIndex = 0;
    this.refMap = {};
    this.lastSnapshot = "";
    this.frameCallback = null;
  }

  // --- Navigation ---

  async navigate(url: string, waitUntil?: "load" | "domcontentloaded" | "networkidle"): Promise<void> {
    const page = this.getPage();
    await page.goto(url, { waitUntil: waitUntil ?? "load" });
  }

  async back(): Promise<void> {
    await this.getPage().goBack();
  }

  async forward(): Promise<void> {
    await this.getPage().goForward();
  }

  async reload(): Promise<void> {
    await this.getPage().reload();
  }

  async setViewport(width: number, height: number): Promise<void> {
    await this.getPage().setViewportSize({ width, height });
  }

  // --- Page interaction ---

  async click(options: {
    selector: string;
    button?: "left" | "right" | "middle";
    clickCount?: number;
    delay?: number;
  }): Promise<void> {
    const locator = this.getLocator(options.selector);
    await locator.click({
      button: options.button,
      clickCount: options.clickCount,
      delay: options.delay,
    });
  }

  async dblclick(selector: string): Promise<void> {
    const locator = this.getLocator(selector);
    await locator.dblclick();
  }

  async fill(selector: string, value: string): Promise<void> {
    const locator = this.getLocator(selector);
    await locator.fill(value);
  }

  async type(options: {
    selector: string;
    text: string;
    delay?: number;
    clear?: boolean;
  }): Promise<void> {
    const locator = this.getLocator(options.selector);
    if (options.clear) {
      await locator.fill("");
    }
    await locator.pressSequentially(options.text, { delay: options.delay });
  }

  async press(key: string, selector?: string): Promise<void> {
    if (selector) {
      await this.getPage().press(selector, key);
    } else {
      await this.getPage().keyboard.press(key);
    }
  }

  async check(selector: string): Promise<void> {
    await this.getLocator(selector).check();
  }

  async uncheck(selector: string): Promise<void> {
    await this.getLocator(selector).uncheck();
  }

  async upload(selector: string, files: string | string[]): Promise<void> {
    const locator = this.getLocator(selector);
    const fileList = Array.isArray(files) ? files : [files];
    await locator.setInputFiles(fileList);
  }

  async focus(selector: string): Promise<void> {
    await this.getLocator(selector).focus();
  }

  async hover(selector: string): Promise<void> {
    await this.getLocator(selector).hover();
  }

  async drag(source: string, target: string): Promise<void> {
    await this.getFrame().dragAndDrop(source, target);
  }

  async select(selector: string, values: string | string[]): Promise<string[]> {
    const locator = this.getLocator(selector);
    const vals = Array.isArray(values) ? values : [values];
    return locator.selectOption(vals);
  }

  async tap(selector: string): Promise<void> {
    await this.getPage().tap(selector);
  }

  async clear(selector: string): Promise<void> {
    await this.getLocator(selector).clear();
  }

  async selectText(selector: string): Promise<void> {
    await this.getLocator(selector).selectText();
  }

  async highlight(selector: string): Promise<void> {
    await this.getLocator(selector).highlight();
  }

  async scrollIntoView(selector: string): Promise<void> {
    await this.getLocator(selector).scrollIntoViewIfNeeded();
  }

  async scroll(options: {
    selector?: string;
    x?: number;
    y?: number;
    direction?: "up" | "down" | "left" | "right";
    amount?: number;
  }): Promise<void> {
    const page = this.getPage();

    if (options.selector) {
      const element = page.locator(options.selector);
      await element.scrollIntoViewIfNeeded();
      if (options.x !== undefined || options.y !== undefined) {
        await element.evaluate(
          (el, { x, y }) => {
            el.scrollBy(x ?? 0, y ?? 0);
          },
          { x: options.x, y: options.y },
        );
      }
    } else {
      let deltaX = options.x ?? 0;
      let deltaY = options.y ?? 0;

      if (options.direction) {
        const amount = options.amount ?? 100;
        switch (options.direction) {
          case "up":
            deltaY = -amount;
            break;
          case "down":
            deltaY = amount;
            break;
          case "left":
            deltaX = -amount;
            break;
          case "right":
            deltaX = amount;
            break;
        }
      }

      await page.evaluate(`window.scrollBy(${deltaX}, ${deltaY})`);
    }
  }

  // --- Content / evaluation ---

  async evaluate(script: string): Promise<unknown> {
    return this.getPage().evaluate(script);
  }

  async evaluateHandle(script: string): Promise<unknown> {
    const handle = await this.getPage().evaluateHandle(script);
    return handle.jsonValue().catch(() => "Handle (non-serializable)");
  }

  async content(selector?: string): Promise<string> {
    const page = this.getPage();
    if (selector) {
      return page.locator(selector).innerHTML();
    }
    return page.content();
  }

  async setContent(html: string): Promise<void> {
    await this.getPage().setContent(html);
  }

  async getUrl(): Promise<string> {
    return this.getPage().url();
  }

  async getTitle(): Promise<string> {
    return this.getPage().title();
  }

  // --- Element queries ---

  async getAttribute(selector: string, attribute: string): Promise<string | null> {
    return this.getLocator(selector).getAttribute(attribute);
  }

  async getText(selector: string): Promise<string | null> {
    return this.getLocator(selector).textContent();
  }

  async innerText(selector: string): Promise<string> {
    return this.getLocator(selector).innerText();
  }

  async innerHTML(selector: string): Promise<string> {
    return this.getLocator(selector).innerHTML();
  }

  async inputValue(selector: string): Promise<string> {
    return this.getLocator(selector).inputValue();
  }

  async isVisible(selector: string): Promise<boolean> {
    return this.getLocator(selector).isVisible();
  }

  async isEnabled(selector: string): Promise<boolean> {
    return this.getLocator(selector).isEnabled();
  }

  async isChecked(selector: string): Promise<boolean> {
    return this.getLocator(selector).isChecked();
  }

  async count(selector: string): Promise<number> {
    return this.getPage().locator(selector).count();
  }

  async boundingBox(selector: string): Promise<{ x: number; y: number; width: number; height: number } | null> {
    return this.getPage().locator(selector).boundingBox();
  }

  async dispatchEvent(selector: string, event: string, eventInit?: Record<string, unknown>): Promise<void> {
    await this.getPage().locator(selector).dispatchEvent(event, eventInit);
  }

  // --- Frames ---

  async switchToFrame(options: { selector?: string; name?: string; url?: string }): Promise<void> {
    const page = this.getPage();

    if (options.selector) {
      const frameElement = await page.$(options.selector);
      if (!frameElement) throw new Error(`Frame not found: ${options.selector}`);
      const frame = await frameElement.contentFrame();
      if (!frame) throw new Error(`Element is not a frame: ${options.selector}`);
      this.activeFrame = frame;
    } else if (options.name) {
      const frame = page.frame({ name: options.name });
      if (!frame) throw new Error(`Frame not found with name: ${options.name}`);
      this.activeFrame = frame;
    } else if (options.url) {
      const frame = page.frame({ url: options.url });
      if (!frame) throw new Error(`Frame not found with URL: ${options.url}`);
      this.activeFrame = frame;
    }
  }

  switchToMainFrame(): void {
    this.activeFrame = null;
  }

  // --- Tabs / windows ---

  async newTab(): Promise<{ index: number; total: number }> {
    if (this.contexts.length === 0) throw new Error("Browser not launched");
    await this.invalidateCDPSession();
    const context = this.contexts[0];
    const page = await context.newPage();
    if (!this.pages.includes(page)) {
      this.pages.push(page);
      this.setupPageTracking(page);
    }
    this.activePageIndex = this.pages.length - 1;
    return { index: this.activePageIndex, total: this.pages.length };
  }

  async newWindow(viewport?: { width: number; height: number }): Promise<{ index: number; total: number }> {
    if (!this.browser) throw new Error("Browser not launched");
    const context = await this.browser.newContext({
      viewport: viewport ?? { width: 1280, height: 720 },
    });
    context.setDefaultTimeout(60000);
    this.contexts.push(context);
    this.setupContextTracking(context);
    const page = await context.newPage();
    if (!this.pages.includes(page)) {
      this.pages.push(page);
      this.setupPageTracking(page);
    }
    this.activePageIndex = this.pages.length - 1;
    return { index: this.activePageIndex, total: this.pages.length };
  }

  async switchTo(index: number): Promise<{ index: number; url: string; title: string }> {
    if (index < 0 || index >= this.pages.length) {
      throw new Error(`Invalid tab index: ${index}. Available: 0-${this.pages.length - 1}`);
    }
    if (index !== this.activePageIndex) {
      await this.invalidateCDPSession();
    }
    this.activePageIndex = index;
    const page = this.pages[index];
    return { index: this.activePageIndex, url: page.url(), title: "" };
  }

  async closeTab(index?: number): Promise<{ closed: number; remaining: number }> {
    const targetIndex = index ?? this.activePageIndex;
    if (targetIndex < 0 || targetIndex >= this.pages.length) {
      throw new Error(`Invalid tab index: ${targetIndex}`);
    }
    if (this.pages.length === 1) {
      throw new Error('Cannot close the last tab. Use "close" to close the browser.');
    }
    if (targetIndex === this.activePageIndex) {
      await this.invalidateCDPSession();
    }
    const page = this.pages[targetIndex];
    await page.close();
    this.pages.splice(targetIndex, 1);
    if (this.activePageIndex >= this.pages.length) {
      this.activePageIndex = this.pages.length - 1;
    } else if (this.activePageIndex > targetIndex) {
      this.activePageIndex--;
    }
    return { closed: targetIndex, remaining: this.pages.length };
  }

  async listTabs(): Promise<Array<{ index: number; url: string; title: string; active: boolean }>> {
    return Promise.all(
      this.pages.map(async (page, index) => ({
        index,
        url: page.url(),
        title: await page.title().catch(() => ""),
        active: index === this.activePageIndex,
      })),
    );
  }

  // --- Dialog ---

  setDialogHandler(response: "accept" | "dismiss", promptText?: string): void {
    const page = this.getPage();
    if (this.dialogHandler) {
      page.removeListener("dialog", this.dialogHandler);
    }
    this.dialogHandler = async (dialog: Dialog) => {
      if (response === "accept") {
        await dialog.accept(promptText);
      } else {
        await dialog.dismiss();
      }
    };
    page.on("dialog", this.dialogHandler);
  }

  // --- Network tracking ---

  startRequestTracking(): void {
    const page = this.getPage();
    page.on("request", (request: Request) => {
      this.trackedRequests.push({
        url: request.url(),
        method: request.method(),
        headers: request.headers(),
        timestamp: Date.now(),
        resourceType: request.resourceType(),
      });
    });
  }

  getRequests(filter?: string): TrackedRequest[] {
    if (filter) return this.trackedRequests.filter((r) => r.url.includes(filter));
    return this.trackedRequests;
  }

  clearRequests(): void {
    this.trackedRequests = [];
  }

  async addRoute(
    url: string,
    options: {
      response?: { status?: number; body?: string; contentType?: string; headers?: Record<string, string> };
      abort?: boolean;
    },
  ): Promise<void> {
    const page = this.getPage();
    const handler = async (route: Route) => {
      if (options.abort) {
        await route.abort();
      } else if (options.response) {
        await route.fulfill({
          status: options.response.status ?? 200,
          body: options.response.body ?? "",
          contentType: options.response.contentType ?? "text/plain",
          headers: options.response.headers,
        });
      } else {
        await route.continue();
      }
    };
    this.routes.set(url, handler);
    await page.route(url, handler);
  }

  async removeRoute(url?: string): Promise<void> {
    const page = this.getPage();
    if (url) {
      const handler = this.routes.get(url);
      if (handler) {
        await page.unroute(url, handler);
        this.routes.delete(url);
      }
    } else {
      for (const [routeUrl, handler] of this.routes) {
        await page.unroute(routeUrl, handler);
      }
      this.routes.clear();
    }
  }

  async setScopedHeaders(origin: string, headers: Record<string, string>): Promise<void> {
    const page = this.getPage();
    let urlPattern: string;
    try {
      const url = new URL(origin.startsWith("http") ? origin : `https://${origin}`);
      urlPattern = `**://${url.host}/**`;
    } catch {
      urlPattern = `**://${origin}/**`;
    }

    const existingHandler = this.scopedHeaderRoutes.get(urlPattern);
    if (existingHandler) {
      await page.unroute(urlPattern, existingHandler);
    }

    const handler = async (route: Route) => {
      const requestHeaders = route.request().headers();
      await route.continue({ headers: { ...requestHeaders, ...headers } });
    };

    this.scopedHeaderRoutes.set(urlPattern, handler);
    await page.route(urlPattern, handler);
  }

  // --- Cookies / storage ---

  async cookiesGet(urls?: string[]): Promise<unknown> {
    return this.getPage().context().cookies(urls);
  }

  async cookiesSet(cookies: Parameters<BrowserContext["addCookies"]>[0]): Promise<void> {
    const page = this.getPage();
    const context = page.context();
    const pageUrl = page.url();
    const normalized = cookies.map((cookie) => {
      if (!cookie.url && !cookie.domain && !cookie.path) {
        return { ...cookie, url: pageUrl };
      }
      return cookie;
    });
    await context.addCookies(normalized);
  }

  async cookiesClear(): Promise<void> {
    await this.getPage().context().clearCookies();
  }

  async storageState(): Promise<string> {
    const context = this.getPage().context();
    const state = await context.storageState();
    return JSON.stringify(state);
  }

  async applyStorageState(storageState: string): Promise<void> {
    const parsed = JSON.parse(storageState);
    if (!this.browser) throw new Error("Browser not launched");
    const viewport = this.getPage().viewportSize() ?? { width: 1280, height: 720 };

    const oldContext = this.getPage().context();
    const contextIndex = this.contexts.indexOf(oldContext);

    // Remove old pages from our tracking
    for (const page of oldContext.pages()) {
      const idx = this.pages.indexOf(page);
      if (idx !== -1) this.pages.splice(idx, 1);
    }

    await oldContext.close().catch(() => {});

    const newContext = await this.browser.newContext({ viewport, storageState: parsed });
    newContext.setDefaultTimeout(60000);

    if (contextIndex !== -1) {
      this.contexts[contextIndex] = newContext;
    } else {
      this.contexts.push(newContext);
    }
    this.setupContextTracking(newContext);

    const page = await newContext.newPage();
    if (!this.pages.includes(page)) {
      this.pages.push(page);
      this.setupPageTracking(page);
    }
    this.activePageIndex = this.pages.length - 1;

    if (this.cdpSession) {
      await this.cdpSession.detach().catch(() => {});
      this.cdpSession = null;
    }
  }

  async saveStorageState(filePath: string): Promise<void> {
    const context = this.getPage().context();
    await context.storageState({ path: filePath });
  }

  // --- localStorage / sessionStorage ---

  async storageGet(type: "local" | "session", key?: string): Promise<unknown> {
    const page = this.getPage();
    const storageType = type === "local" ? "localStorage" : "sessionStorage";

    if (key) {
      return page.evaluate(`${storageType}.getItem(${JSON.stringify(key)})`);
    }
    return page.evaluate(`
      (() => {
        const storage = ${storageType};
        const result = {};
        for (let i = 0; i < storage.length; i++) {
          const key = storage.key(i);
          if (key) result[key] = storage.getItem(key);
        }
        return result;
      })()
    `);
  }

  async storageSet(type: "local" | "session", key: string, value: string): Promise<void> {
    const page = this.getPage();
    const storageType = type === "local" ? "localStorage" : "sessionStorage";
    await page.evaluate(
      `${storageType}.setItem(${JSON.stringify(key)}, ${JSON.stringify(value)})`,
    );
  }

  async storageClear(type: "local" | "session"): Promise<void> {
    const page = this.getPage();
    const storageType = type === "local" ? "localStorage" : "sessionStorage";
    await page.evaluate(`${storageType}.clear()`);
  }

  // --- Wait ---

  async wait(options: {
    selector?: string;
    timeout?: number;
    state?: "attached" | "detached" | "visible" | "hidden";
  }): Promise<void> {
    const page = this.getPage();
    if (options.selector) {
      await page.waitForSelector(options.selector, {
        state: options.state ?? "visible",
        timeout: options.timeout,
      });
    } else if (options.timeout) {
      await page.waitForTimeout(options.timeout);
    } else {
      await page.waitForLoadState("load");
    }
  }

  async waitForUrl(url: string, timeout?: number): Promise<void> {
    await this.getPage().waitForURL(url, { timeout });
  }

  async waitForLoadState(state: "load" | "domcontentloaded" | "networkidle", timeout?: number): Promise<void> {
    await this.getPage().waitForLoadState(state, { timeout });
  }

  async waitForFunction(expression: string, timeout?: number): Promise<void> {
    await this.getPage().waitForFunction(expression, { timeout });
  }

  // --- Screenshot / PDF ---

  async screenshot(options: {
    path?: string;
    fullPage?: boolean;
    selector?: string;
    format?: "png" | "jpeg";
    quality?: number;
  }): Promise<string> {
    const page = this.getPage();
    const screenshotOptions: Parameters<Page["screenshot"]>[0] = {
      fullPage: options.fullPage,
      type: options.format ?? "png",
    };
    if (options.format === "jpeg" && options.quality !== undefined) {
      screenshotOptions.quality = options.quality;
    }

    let target: Page | Locator = page;
    if (options.selector) {
      target = this.getLocator(options.selector);
    }

    let savePath = options.path;
    if (!savePath) {
      const ext = options.format === "jpeg" ? "jpg" : "png";
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
      const random = Math.random().toString(36).substring(2, 8);
      const filename = `screenshot-${timestamp}-${random}.${ext}`;
      const screenshotDir = path.join(os.tmpdir(), "octo-browserd", "screenshots");
      mkdirSync(screenshotDir, { recursive: true });
      savePath = path.join(screenshotDir, filename);
    }

    await target.screenshot({ ...screenshotOptions, path: savePath });
    return savePath;
  }

  async pdf(filePath: string, format?: string): Promise<void> {
    const page = this.getPage();
    await page.pdf({ path: filePath, format: (format as "Letter") ?? "Letter" });
  }

  // --- Console / errors ---

  startConsoleTracking(): void {
    const page = this.getPage();
    page.on("console", (msg) => {
      this.consoleMessages.push({ type: msg.type(), text: msg.text(), timestamp: Date.now() });
    });
  }

  getConsoleMessages(): ConsoleMessage[] {
    return this.consoleMessages;
  }

  clearConsoleMessages(): void {
    this.consoleMessages = [];
  }

  startErrorTracking(): void {
    const page = this.getPage();
    page.on("pageerror", (error) => {
      this.pageErrors.push({ message: error.message, timestamp: Date.now() });
    });
  }

  getPageErrors(): PageError[] {
    return this.pageErrors;
  }

  clearPageErrors(): void {
    this.pageErrors = [];
  }

  // --- Geolocation / permissions / emulation ---

  async setGeolocation(latitude: number, longitude: number, accuracy?: number): Promise<void> {
    const context = this.contexts[0];
    if (context) await context.setGeolocation({ latitude, longitude, accuracy });
  }

  async setPermissions(permissions: string[], grant: boolean): Promise<void> {
    const context = this.contexts[0];
    if (context) {
      if (grant) {
        await context.grantPermissions(permissions);
      } else {
        await context.clearPermissions();
      }
    }
  }

  async setOffline(offline: boolean): Promise<void> {
    const context = this.contexts[0];
    if (context) await context.setOffline(offline);
  }

  async setExtraHeaders(headers: Record<string, string>): Promise<void> {
    const context = this.contexts[0];
    if (context) await context.setExtraHTTPHeaders(headers);
  }

  async setHttpCredentials(username: string, password: string): Promise<void> {
    const context = this.getPage().context();
    await context.setHTTPCredentials({ username, password });
  }

  async emulateMedia(options: {
    media?: "screen" | "print" | null;
    colorScheme?: "light" | "dark" | "no-preference" | null;
    reducedMotion?: "reduce" | "no-preference" | null;
    forcedColors?: "active" | "none" | null;
  }): Promise<void> {
    await this.getPage().emulateMedia(options);
  }

  async addInitScript(script: string): Promise<void> {
    await this.getPage().context().addInitScript(script);
  }

  async addScriptTag(options: { content?: string; url?: string }): Promise<void> {
    await this.getPage().addScriptTag(options);
  }

  async addStyleTag(options: { content?: string; url?: string }): Promise<void> {
    await this.getPage().addStyleTag(options);
  }

  async exposeFunction(name: string): Promise<void> {
    await this.getPage().exposeFunction(name, () => `Function ${name} called`);
  }

  async bringToFront(): Promise<void> {
    await this.getPage().bringToFront();
  }

  // --- Device emulation ---

  getDevice(deviceName: string): (typeof devices)[keyof typeof devices] | undefined {
    return devices[deviceName as keyof typeof devices];
  }

  listDevices(): string[] {
    return Object.keys(devices);
  }

  async setDeviceScaleFactor(
    deviceScaleFactor: number,
    width: number,
    height: number,
    mobile = false,
  ): Promise<void> {
    const cdp = await this.getCDPSession();
    await cdp.send("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor,
      mobile,
    });
  }

  async clearDeviceMetricsOverride(): Promise<void> {
    const cdp = await this.getCDPSession();
    await cdp.send("Emulation.clearDeviceMetricsOverride");
  }

  // --- HAR / Tracing ---

  async startHarRecording(): Promise<void> {
    this.isRecordingHar = true;
  }

  isHarRecording(): boolean {
    return this.isRecordingHar;
  }

  async startTracing(options: { screenshots?: boolean; snapshots?: boolean }): Promise<void> {
    const context = this.contexts[0];
    if (context) {
      await context.tracing.start({
        screenshots: options.screenshots ?? true,
        snapshots: options.snapshots ?? true,
      });
    }
  }

  async stopTracing(filePath: string): Promise<void> {
    const context = this.contexts[0];
    if (context) {
      await context.tracing.stop({ path: filePath });
    }
  }

  // --- Keyboard / mouse / touch (high-level Playwright API) ---

  async keyboardPress(keys: string): Promise<void> {
    await this.getPage().keyboard.press(keys);
  }

  async keyDown(key: string): Promise<void> {
    await this.getPage().keyboard.down(key);
  }

  async keyUp(key: string): Promise<void> {
    await this.getPage().keyboard.up(key);
  }

  async insertText(text: string): Promise<void> {
    await this.getPage().keyboard.insertText(text);
  }

  async mouseMove(x: number, y: number): Promise<void> {
    await this.getPage().mouse.move(x, y);
  }

  async mouseDown(button?: "left" | "right" | "middle"): Promise<void> {
    await this.getPage().mouse.down({ button: button ?? "left" });
  }

  async mouseUp(button?: "left" | "right" | "middle"): Promise<void> {
    await this.getPage().mouse.up({ button: button ?? "left" });
  }

  async wheel(deltaX: number, deltaY: number, selector?: string): Promise<void> {
    const page = this.getPage();
    if (selector) {
      await page.locator(selector).hover();
    }
    await page.mouse.wheel(deltaX, deltaY);
  }

  // --- Clipboard ---

  async clipboardCopy(): Promise<void> {
    await this.getPage().keyboard.press("Control+c");
  }

  async clipboardPaste(): Promise<void> {
    await this.getPage().keyboard.press("Control+v");
  }

  async clipboardRead(): Promise<unknown> {
    return this.getPage().evaluate("navigator.clipboard.readText()");
  }

  // --- Computed styles ---

  async getStyles(selector: string): Promise<unknown[]> {
    const page = this.getPage();

    const extractStylesScript = `(function(el) {
      const s = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      return {
        tag: el.tagName.toLowerCase(),
        text: el.innerText?.trim().slice(0, 80) || null,
        box: {
          x: Math.round(r.x),
          y: Math.round(r.y),
          width: Math.round(r.width),
          height: Math.round(r.height),
        },
        styles: {
          fontSize: s.fontSize,
          fontWeight: s.fontWeight,
          fontFamily: s.fontFamily.split(',')[0].trim().replace(/"/g, ''),
          color: s.color,
          backgroundColor: s.backgroundColor,
          borderRadius: s.borderRadius,
          border: s.border !== 'none' && s.borderWidth !== '0px' ? s.border : null,
          boxShadow: s.boxShadow !== 'none' ? s.boxShadow : null,
          padding: s.padding,
        },
      };
    })`;

    if (this.isRef(selector)) {
      const locator = this.getLocator(selector);
      const element = await locator.evaluate((el, script) => {
        const fn = eval(script);
        return fn(el);
      }, extractStylesScript);
      return [element];
    }

    return page.$$eval(
      selector,
      (els, script) => {
        const fn = eval(script);
        return els.map((el) => fn(el));
      },
      extractStylesScript,
    );
  }

  // --- Recording (Playwright native video) ---

  async startRecording(outputPath: string, url?: string): Promise<void> {
    if (this.recordingContext) {
      throw new Error(
        "Recording already in progress. Run 'recording_stop' first, or use 'recording_restart'.",
      );
    }
    if (!this.browser) throw new Error("Browser not launched");
    if (existsSync(outputPath)) throw new Error(`Output file already exists: ${outputPath}`);
    if (!outputPath.endsWith(".webm")) {
      throw new Error("Playwright native recording only supports WebM format (.webm extension).");
    }

    const currentPage = this.pages.length > 0 ? this.pages[this.activePageIndex] : null;
    const currentContext = this.contexts.length > 0 ? this.contexts[0] : null;
    if (!url && currentPage) {
      const currentUrl = currentPage.url();
      if (currentUrl && currentUrl !== "about:blank") {
        url = currentUrl;
      }
    }

    let storageStateData: Awaited<ReturnType<BrowserContext["storageState"]>> | undefined;
    if (currentContext) {
      try {
        storageStateData = await currentContext.storageState();
      } catch {
        // ignore
      }
    }

    const session = process.env.AGENT_BROWSER_SESSION || "default";
    this.recordingTempDir = path.join(os.tmpdir(), `octo-browserd-recording-${session}-${Date.now()}`);
    mkdirSync(this.recordingTempDir, { recursive: true });
    this.recordingOutputPath = outputPath;

    const viewport = { width: 1280, height: 720 };
    this.recordingContext = await this.browser.newContext({
      viewport,
      recordVideo: { dir: this.recordingTempDir, size: viewport },
      storageState: storageStateData,
    });
    this.recordingContext.setDefaultTimeout(10000);

    this.recordingPage = await this.recordingContext.newPage();
    this.contexts.push(this.recordingContext);
    this.pages.push(this.recordingPage);
    this.activePageIndex = this.pages.length - 1;
    this.setupPageTracking(this.recordingPage);
    await this.invalidateCDPSession();

    if (url) {
      await this.recordingPage.goto(url, { waitUntil: "load" });
    }
  }

  async stopRecording(): Promise<{ path: string; frames: number; error?: string }> {
    if (!this.recordingContext || !this.recordingPage) {
      return { path: "", frames: 0, error: "No recording in progress" };
    }

    const outputPath = this.recordingOutputPath;

    try {
      const video = this.recordingPage.video();

      const pageIndex = this.pages.indexOf(this.recordingPage);
      if (pageIndex !== -1) this.pages.splice(pageIndex, 1);
      const contextIndex = this.contexts.indexOf(this.recordingContext);
      if (contextIndex !== -1) this.contexts.splice(contextIndex, 1);

      await this.recordingPage.close();
      if (video) await video.saveAs(outputPath);
      if (this.recordingTempDir) rmSync(this.recordingTempDir, { recursive: true, force: true });
      await this.recordingContext.close();

      this.recordingContext = null;
      this.recordingPage = null;
      this.recordingOutputPath = "";
      this.recordingTempDir = "";

      if (this.pages.length > 0) {
        this.activePageIndex = Math.min(this.activePageIndex, this.pages.length - 1);
      } else {
        this.activePageIndex = 0;
      }
      await this.invalidateCDPSession();
      return { path: outputPath, frames: 0 };
    } catch (error) {
      if (this.recordingTempDir) rmSync(this.recordingTempDir, { recursive: true, force: true });
      this.recordingContext = null;
      this.recordingPage = null;
      this.recordingOutputPath = "";
      this.recordingTempDir = "";

      const message = error instanceof Error ? error.message : String(error);
      return { path: outputPath, frames: 0, error: message };
    }
  }

  async restartRecording(
    outputPath: string,
    url?: string,
  ): Promise<{ previousPath?: string; stopped: boolean }> {
    let previousPath: string | undefined;
    let stopped = false;

    if (this.recordingContext) {
      const result = await this.stopRecording();
      previousPath = result.path;
      stopped = true;
    }

    await this.startRecording(outputPath, url);
    return { previousPath, stopped };
  }

  // --- CDP session for screencast and input injection ---

  async getCDPSession(): Promise<CDPSession> {
    if (this.cdpSession) return this.cdpSession;
    const page = this.getPage();
    const context = page.context();
    this.cdpSession = await context.newCDPSession(page);
    return this.cdpSession;
  }

  private async invalidateCDPSession(): Promise<void> {
    if (this.screencastActive) {
      await this.stopScreencast();
    }
    if (this.cdpSession) {
      await this.cdpSession.detach().catch(() => {});
      this.cdpSession = null;
    }
  }

  // --- Screencast ---

  async startScreencast(
    callback: (frame: ScreencastFrame) => void,
    options: ScreencastOptions,
  ): Promise<void> {
    if (this.screencastActive) {
      throw new Error("Screencast already active");
    }

    const cdp = await this.getCDPSession();
    this.frameCallback = callback;
    this.screencastActive = true;

    this.screencastFrameHandler = async (params: unknown) => {
      const p = params as { data: string; metadata: ScreencastFrame["metadata"]; sessionId: number };
      const frame: ScreencastFrame = {
        data: p.data,
        metadata: p.metadata,
        sessionId: p.sessionId,
      };
      await cdp.send("Page.screencastFrameAck", { sessionId: p.sessionId });
      if (this.frameCallback) this.frameCallback(frame);
    };

    cdp.on("Page.screencastFrame", this.screencastFrameHandler);

    await cdp.send("Page.startScreencast", {
      format: options.format,
      quality: options.quality,
      maxWidth: options.maxWidth,
      maxHeight: options.maxHeight,
      everyNthFrame: options.everyNthFrame,
    });
  }

  async stopScreencast(): Promise<void> {
    if (!this.screencastActive) return;

    try {
      const cdp = await this.getCDPSession();
      await cdp.send("Page.stopScreencast");
      if (this.screencastFrameHandler) {
        cdp.off("Page.screencastFrame", this.screencastFrameHandler);
      }
    } catch {
      // ignore
    }

    this.screencastActive = false;
    this.frameCallback = null;
    this.screencastFrameHandler = null;
  }

  // --- CDP input injection ---

  async injectMouseEvent(params: {
    type: "mousePressed" | "mouseReleased" | "mouseMoved" | "mouseWheel";
    x: number;
    y: number;
    button?: "left" | "right" | "middle" | "none";
    clickCount?: number;
    deltaX?: number;
    deltaY?: number;
    modifiers?: number;
  }): Promise<void> {
    const cdp = await this.getCDPSession();
    await cdp.send("Input.dispatchMouseEvent", {
      type: params.type,
      x: params.x,
      y: params.y,
      button: params.button ?? "left",
      clickCount: params.clickCount ?? 1,
      deltaX: params.deltaX ?? 0,
      deltaY: params.deltaY ?? 0,
      modifiers: params.modifiers ?? 0,
    });
  }

  async injectKeyboardEvent(params: {
    type: "keyDown" | "keyUp" | "char";
    key?: string;
    code?: string;
    text?: string;
    keyCode?: number;
    modifiers?: number;
  }): Promise<void> {
    const cdp = await this.getCDPSession();
    const payload: {
      type: "keyDown" | "keyUp" | "char";
      key?: string;
      code?: string;
      modifiers: number;
      text?: string;
      unmodifiedText?: string;
      windowsVirtualKeyCode?: number;
      nativeVirtualKeyCode?: number;
    } = {
      type: params.type,
      key: params.key,
      code: params.code,
      modifiers: params.modifiers ?? 0,
    };

    if (params.text !== undefined) {
      payload.text = params.text;
      payload.unmodifiedText = params.text;
    }

    if (params.keyCode !== undefined) {
      payload.windowsVirtualKeyCode = params.keyCode;
      payload.nativeVirtualKeyCode = params.keyCode;
    }

    await cdp.send("Input.dispatchKeyEvent", payload);
  }

  async injectTouchEvent(params: {
    type: "touchStart" | "touchEnd" | "touchMove" | "touchCancel";
    touchPoints: Array<{ x: number; y: number; id?: number }>;
    modifiers?: number;
  }): Promise<void> {
    const cdp = await this.getCDPSession();
    await cdp.send("Input.dispatchTouchEvent", {
      type: params.type,
      touchPoints: params.touchPoints.map((tp, i) => ({
        x: tp.x,
        y: tp.y,
        id: tp.id ?? i,
      })),
      modifiers: params.modifiers ?? 0,
    });
  }

  // --- Internal helpers ---

  private setupPageTracking(page: Page): void {
    page.on("console", (msg) => {
      this.consoleMessages.push({ type: msg.type(), text: msg.text(), timestamp: Date.now() });
    });
    page.on("pageerror", (error) => {
      this.pageErrors.push({ message: error.message, timestamp: Date.now() });
    });
    page.on("close", () => {
      const index = this.pages.indexOf(page);
      if (index !== -1) {
        this.pages.splice(index, 1);
        if (this.activePageIndex >= this.pages.length) {
          this.activePageIndex = Math.max(0, this.pages.length - 1);
        }
      }
    });
  }

  private setupContextTracking(context: BrowserContext): void {
    context.on("page", (page) => {
      if (!this.pages.includes(page)) {
        this.pages.push(page);
        this.setupPageTracking(page);
      }
      const newIndex = this.pages.indexOf(page);
      if (newIndex !== -1 && newIndex !== this.activePageIndex) {
        this.activePageIndex = newIndex;
        this.invalidateCDPSession().catch(() => {});
      }
    });
  }
}
