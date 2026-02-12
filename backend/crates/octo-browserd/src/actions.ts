import type { BrowserManager, ScreencastFrame } from "./browser.js";
import {
  successResponse,
  errorResponse,
  type Command,
  type ResponsePayload,
  type NavigateCommand,
  type ClickCommand,
  type TypeCommand,
  type FillCommand,
  type PressCommand,
  type CheckCommand,
  type UncheckCommand,
  type UploadCommand,
  type DoubleClickCommand,
  type FocusCommand,
  type DragCommand,
  type HoverCommand,
  type SelectCommand,
  type TapCommand,
  type ClearCommand,
  type SelectAllCommand,
  type HighlightCommand,
  type ScrollIntoViewCommand,
  type ScrollCommand,
  type FrameCommand,
  type GetByRoleCommand,
  type GetByTextCommand,
  type GetByLabelCommand,
  type GetByPlaceholderCommand,
  type GetByAltTextCommand,
  type GetByTitleCommand,
  type GetByTestIdCommand,
  type NthCommand,
  type SnapshotCommand,
  type EvaluateCommand,
  type ContentCommand,
  type SetContentCommand,
  type ScreenshotCommand,
  type PdfCommand,
  type WaitCommand,
  type WaitForUrlCommand,
  type WaitForLoadStateCommand,
  type WaitForFunctionCommand,
  type WaitForDownloadCommand,
  type GetAttributeCommand,
  type GetTextCommand,
  type InnerTextCommand,
  type InnerHtmlCommand,
  type InputValueCommand,
  type SetValueCommand,
  type IsVisibleCommand,
  type IsEnabledCommand,
  type IsCheckedCommand,
  type CountCommand,
  type BoundingBoxCommand,
  type StylesCommand,
  type DispatchEventCommand,
  type ViewportCommand,
  type TabNewCommand,
  type TabSwitchCommand,
  type TabCloseCommand,
  type WindowNewCommand,
  type CookiesSetCommand,
  type StorageGetCommand,
  type StorageSetCommand,
  type StorageClearCommand,
  type StorageStateSaveCommand,
  type DialogCommand,
  type RouteCommand,
  type RequestsCommand,
  type DownloadCommand,
  type ResponseBodyCommand,
  type HeadersCommand,
  type GeolocationCommand,
  type PermissionsCommand,
  type DeviceCommand,
  type EmulateMediaCommand,
  type OfflineCommand,
  type HttpCredentialsCommand,
  type ConsoleCommand,
  type ErrorsCommand,
  type KeyboardCommand,
  type KeyDownCommand,
  type KeyUpCommand,
  type InsertTextCommand,
  type MouseMoveCommand,
  type MouseDownCommand,
  type MouseUpCommand,
  type WheelCommand,
  type ClipboardCommand,
  type MultiSelectCommand,
  type AddScriptCommand,
  type AddStyleCommand,
  type AddInitScriptCommand,
  type ScreencastStartCommand,
  type ScreencastStopCommand,
  type InputMouseCommand,
  type InputKeyboardCommand,
  type InputTouchCommand,
  type RecordingStartCommand,
  type RecordingStopCommand,
  type RecordingRestartCommand,
  type TraceStartCommand,
  type TraceStopCommand,
  type HarStopCommand,
  type EvalHandleCommand,
} from "./protocol.js";

// Screencast frame callback - set by daemon when streaming is active
let screencastFrameCallback: ((frame: ScreencastFrame) => void) | null = null;

export function setScreencastFrameCallback(
  callback: ((frame: ScreencastFrame) => void) | null,
): void {
  screencastFrameCallback = callback;
}

/**
 * Convert Playwright errors to AI-friendly messages
 */
export function toAIFriendlyError(error: unknown, selector: string): Error {
  const message = error instanceof Error ? error.message : String(error);

  if (message.includes("strict mode violation")) {
    const countMatch = message.match(/resolved to (\d+) elements/);
    const count = countMatch ? countMatch[1] : "multiple";
    return new Error(
      `Selector "${selector}" matched ${count} elements. ` +
        `Run 'snapshot' to get updated refs, or use a more specific CSS selector.`,
    );
  }

  if (message.includes("intercepts pointer events")) {
    return new Error(
      `Element "${selector}" is blocked by another element (likely a modal or overlay). ` +
        `Try dismissing any modals/cookie banners first.`,
    );
  }

  if (message.includes("not visible") && !message.includes("Timeout")) {
    return new Error(
      `Element "${selector}" is not visible. ` +
        `Try scrolling it into view or check if it's hidden.`,
    );
  }

  if (message.includes("Timeout") && message.includes("exceeded")) {
    return new Error(
      `Action on "${selector}" timed out. The element may be blocked, still loading, or not interactable. ` +
        `Run 'snapshot' to check the current page state.`,
    );
  }

  if (
    message.includes("waiting for") &&
    (message.includes("to be visible") || message.includes("Timeout"))
  ) {
    return new Error(
      `Element "${selector}" not found or not visible. ` +
        `Run 'snapshot' to see current page elements.`,
    );
  }

  return error instanceof Error ? error : new Error(message);
}

export async function executeCommand(
  command: Command,
  browser: BrowserManager,
): Promise<ResponsePayload> {
  try {
    switch (command.action) {
      case "launch":
        return handleLaunch(command, browser);
      case "navigate":
        return handleNavigate(command as NavigateCommand, browser);
      case "click":
        return handleClick(command as ClickCommand, browser);
      case "type":
        return handleType(command as TypeCommand, browser);
      case "fill":
        return handleFill(command as FillCommand, browser);
      case "press":
        return handlePress(command as PressCommand, browser);
      case "check":
        return handleCheck(command as CheckCommand, browser);
      case "uncheck":
        return handleUncheck(command as UncheckCommand, browser);
      case "upload":
        return handleUpload(command as UploadCommand, browser);
      case "dblclick":
        return handleDoubleClick(command as DoubleClickCommand, browser);
      case "focus":
        return handleFocus(command as FocusCommand, browser);
      case "drag":
        return handleDrag(command as DragCommand, browser);
      case "hover":
        return handleHover(command as HoverCommand, browser);
      case "select":
        return handleSelect(command as SelectCommand, browser);
      case "tap":
        return handleTap(command as TapCommand, browser);
      case "clear":
        return handleClear(command as ClearCommand, browser);
      case "selectall":
        return handleSelectAll(command as SelectAllCommand, browser);
      case "highlight":
        return handleHighlight(command as HighlightCommand, browser);
      case "scrollintoview":
        return handleScrollIntoView(command as ScrollIntoViewCommand, browser);
      case "scroll":
        return handleScroll(command as ScrollCommand, browser);
      case "frame":
        return handleFrame(command as FrameCommand, browser);
      case "mainframe":
        return handleMainFrame(command, browser);
      case "getbyrole":
        return handleGetByRole(command as GetByRoleCommand, browser);
      case "getbytext":
        return handleGetByText(command as GetByTextCommand, browser);
      case "getbylabel":
        return handleGetByLabel(command as GetByLabelCommand, browser);
      case "getbyplaceholder":
        return handleGetByPlaceholder(command as GetByPlaceholderCommand, browser);
      case "getbyalttext":
        return handleGetByAltText(command as GetByAltTextCommand, browser);
      case "getbytitle":
        return handleGetByTitle(command as GetByTitleCommand, browser);
      case "getbytestid":
        return handleGetByTestId(command as GetByTestIdCommand, browser);
      case "nth":
        return handleNth(command as NthCommand, browser);
      case "snapshot":
        return handleSnapshot(command as SnapshotCommand, browser);
      case "evaluate":
        return handleEvaluate(command as EvaluateCommand, browser);
      case "evalhandle":
        return handleEvalHandle(command as EvalHandleCommand, browser);
      case "content":
        return handleContent(command as ContentCommand, browser);
      case "setcontent":
        return handleSetContent(command as SetContentCommand, browser);
      case "screenshot":
        return handleScreenshot(command as ScreenshotCommand, browser);
      case "pdf":
        return handlePdf(command as PdfCommand, browser);
      case "wait":
        return handleWait(command as WaitCommand, browser);
      case "waitforurl":
        return handleWaitForUrl(command as WaitForUrlCommand, browser);
      case "waitforloadstate":
        return handleWaitForLoadState(command as WaitForLoadStateCommand, browser);
      case "waitforfunction":
        return handleWaitForFunction(command as WaitForFunctionCommand, browser);
      case "waitfordownload":
        return handleWaitForDownload(command as WaitForDownloadCommand, browser);
      case "getattribute":
        return handleGetAttribute(command as GetAttributeCommand, browser);
      case "gettext":
        return handleGetText(command as GetTextCommand, browser);
      case "innertext":
        return handleInnerText(command as InnerTextCommand, browser);
      case "innerhtml":
        return handleInnerHtml(command as InnerHtmlCommand, browser);
      case "inputvalue":
        return handleInputValue(command as InputValueCommand, browser);
      case "setvalue":
        return handleSetValue(command as SetValueCommand, browser);
      case "isvisible":
        return handleIsVisible(command as IsVisibleCommand, browser);
      case "isenabled":
        return handleIsEnabled(command as IsEnabledCommand, browser);
      case "ischecked":
        return handleIsChecked(command as IsCheckedCommand, browser);
      case "count":
        return handleCount(command as CountCommand, browser);
      case "boundingbox":
        return handleBoundingBox(command as BoundingBoxCommand, browser);
      case "styles":
        return handleStyles(command as StylesCommand, browser);
      case "dispatch":
        return handleDispatch(command as DispatchEventCommand, browser);
      case "viewport":
        return handleViewport(command as ViewportCommand, browser);
      case "back":
        return handleBack(command, browser);
      case "forward":
        return handleForward(command, browser);
      case "reload":
        return handleReload(command, browser);
      case "close":
        return handleClose(command, browser);
      case "url":
        return handleUrl(command, browser);
      case "title":
        return handleTitle(command, browser);
      case "tab_new":
        return handleTabNew(command as TabNewCommand, browser);
      case "tab_list":
        return handleTabList(command, browser);
      case "tab_switch":
        return handleTabSwitch(command as TabSwitchCommand, browser);
      case "tab_close":
        return handleTabClose(command as TabCloseCommand, browser);
      case "window_new":
        return handleWindowNew(command as WindowNewCommand, browser);
      case "cookies_get":
        return handleCookiesGet(command, browser);
      case "cookies_set":
        return handleCookiesSet(command as CookiesSetCommand, browser);
      case "cookies_clear":
        return handleCookiesClear(command, browser);
      case "storage_get":
        return handleStorageGet(command as StorageGetCommand, browser);
      case "storage_set":
        return handleStorageSet(command as StorageSetCommand, browser);
      case "storage_clear":
        return handleStorageClear(command as StorageClearCommand, browser);
      case "storage_state":
        return handleStorageState(command, browser);
      case "storage_state_set":
        return handleStorageStateSet(command, browser);
      case "state_save":
        return handleStateSave(command as StorageStateSaveCommand, browser);
      case "state_load":
        return handleStateLoad(command, browser);
      case "dialog":
        return handleDialog(command as DialogCommand, browser);
      case "route":
        return handleRoute(command as RouteCommand, browser);
      case "unroute":
        return handleUnroute(command, browser);
      case "requests":
        return handleRequests(command as RequestsCommand, browser);
      case "download":
        return handleDownload(command as DownloadCommand, browser);
      case "responsebody":
        return handleResponseBody(command as ResponseBodyCommand, browser);
      case "headers":
        return handleHeaders(command as HeadersCommand, browser);
      case "geolocation":
        return handleGeolocation(command as GeolocationCommand, browser);
      case "permissions":
        return handlePermissions(command as PermissionsCommand, browser);
      case "device":
        return handleDevice(command as DeviceCommand, browser);
      case "useragent":
        return handleUserAgent(command, browser);
      case "emulatemedia":
        return handleEmulateMedia(command as EmulateMediaCommand, browser);
      case "offline":
        return handleOffline(command as OfflineCommand, browser);
      case "timezone":
        return handleTimezone(command, browser);
      case "locale":
        return handleLocale(command, browser);
      case "credentials":
        return handleCredentials(command as HttpCredentialsCommand, browser);
      case "console":
        return handleConsole(command as ConsoleCommand, browser);
      case "errors":
        return handleErrors(command as ErrorsCommand, browser);
      case "keyboard":
        return handleKeyboard(command as KeyboardCommand, browser);
      case "keydown":
        return handleKeyDown(command as KeyDownCommand, browser);
      case "keyup":
        return handleKeyUp(command as KeyUpCommand, browser);
      case "inserttext":
        return handleInsertText(command as InsertTextCommand, browser);
      case "mousemove":
        return handleMouseMove(command as MouseMoveCommand, browser);
      case "mousedown":
        return handleMouseDown(command as MouseDownCommand, browser);
      case "mouseup":
        return handleMouseUp(command as MouseUpCommand, browser);
      case "wheel":
        return handleWheel(command as WheelCommand, browser);
      case "clipboard":
        return handleClipboard(command as ClipboardCommand, browser);
      case "multiselect":
        return handleMultiSelect(command as MultiSelectCommand, browser);
      case "addscript":
        return handleAddScript(command as AddScriptCommand, browser);
      case "addstyle":
        return handleAddStyle(command as AddStyleCommand, browser);
      case "addinitscript":
        return handleAddInitScript(command as AddInitScriptCommand, browser);
      case "expose":
        return handleExpose(command, browser);
      case "video_start":
        return handleVideoStart(command, browser);
      case "video_stop":
        return handleVideoStop(command, browser);
      case "recording_start":
        return handleRecordingStart(command as RecordingStartCommand, browser);
      case "recording_stop":
        return handleRecordingStop(command as RecordingStopCommand, browser);
      case "recording_restart":
        return handleRecordingRestart(command as RecordingRestartCommand, browser);
      case "trace_start":
        return handleTraceStart(command as TraceStartCommand, browser);
      case "trace_stop":
        return handleTraceStop(command as TraceStopCommand, browser);
      case "har_start":
        return handleHarStart(command, browser);
      case "har_stop":
        return handleHarStop(command as HarStopCommand, browser);
      case "screencast_start":
        return handleScreencastStart(command as ScreencastStartCommand, browser);
      case "screencast_stop":
        return handleScreencastStop(command as ScreencastStopCommand, browser);
      case "input_mouse":
        return handleInputMouse(command as InputMouseCommand, browser);
      case "input_keyboard":
        return handleInputKeyboard(command as InputKeyboardCommand, browser);
      case "input_touch":
        return handleInputTouch(command as InputTouchCommand, browser);
      case "bringtofront":
        return handleBringToFront(command, browser);
      case "pause":
        return handlePause(command, browser);
      default: {
        const unknownCommand = command as { id: string; action: string };
        return errorResponse(unknownCommand.id, `Unknown action: ${unknownCommand.action}`);
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return errorResponse(command.id, message);
  }
}

// --- Handlers ---

async function handleLaunch(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.launch(command as any);
  return successResponse(command.id, { launched: true });
}

async function handleNavigate(command: NavigateCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  if (command.headers && Object.keys(command.headers).length > 0) {
    await browser.setScopedHeaders(command.url, command.headers);
  }
  await page.goto(command.url, { waitUntil: command.waitUntil ?? "load" });
  return successResponse(command.id, { url: page.url(), title: await page.title() });
}

async function handleClick(command: ClickCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.click({ selector: command.selector, button: command.button, clickCount: command.clickCount, delay: command.delay });
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { clicked: true });
}

async function handleType(command: TypeCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.type({ selector: command.selector, text: command.text, delay: command.delay, clear: command.clear });
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { typed: true });
}

async function handleFill(command: FillCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.fill(command.selector, command.value);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { filled: true });
}

async function handlePress(command: PressCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.press(command.key, command.selector);
  return successResponse(command.id, { pressed: true });
}

async function handleCheck(command: CheckCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.check(command.selector);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { checked: true });
}

async function handleUncheck(command: UncheckCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.uncheck(command.selector);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { unchecked: true });
}

async function handleUpload(command: UploadCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const files = Array.isArray(command.files) ? command.files : [command.files];
  try {
    await browser.upload(command.selector, files);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { uploaded: files });
}

async function handleDoubleClick(command: DoubleClickCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.dblclick(command.selector);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { clicked: true });
}

async function handleFocus(command: FocusCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.focus(command.selector);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { focused: true });
}

async function handleDrag(command: DragCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.drag(command.source, command.target);
  return successResponse(command.id, { dragged: true });
}

async function handleHover(command: HoverCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    await browser.hover(command.selector);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { hovered: true });
}

async function handleSelect(command: SelectCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const values = Array.isArray(command.values) ? command.values : [command.values];
  try {
    await browser.select(command.selector, values);
  } catch (error) {
    throw toAIFriendlyError(error, command.selector);
  }
  return successResponse(command.id, { selected: values });
}

async function handleTap(command: TapCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.tap(command.selector);
  return successResponse(command.id, { tapped: true });
}

async function handleClear(command: ClearCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.clear(command.selector);
  return successResponse(command.id, { cleared: true });
}

async function handleSelectAll(command: SelectAllCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.selectText(command.selector);
  return successResponse(command.id, { selected: true });
}

async function handleHighlight(command: HighlightCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.highlight(command.selector);
  return successResponse(command.id, { highlighted: true });
}

async function handleScrollIntoView(command: ScrollIntoViewCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.scrollIntoView(command.selector);
  return successResponse(command.id, { scrolled: true });
}

async function handleScroll(command: ScrollCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.scroll({ selector: command.selector, x: command.x, y: command.y, direction: command.direction, amount: command.amount });
  return successResponse(command.id, { scrolled: true });
}

async function handleFrame(command: FrameCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.switchToFrame({ selector: command.selector, name: command.name, url: command.url });
  return successResponse(command.id, { switched: true });
}

async function handleMainFrame(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  browser.switchToMainFrame();
  return successResponse(command.id, { switched: true });
}

// --- Semantic locators ---

async function handleGetByRole(command: GetByRoleCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByRole(command.role as any, { name: command.name, exact: command.exact });
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "fill": await locator.fill(command.value ?? ""); return successResponse(command.id, { filled: true });
    case "check": await locator.check(); return successResponse(command.id, { checked: true });
    case "hover": await locator.hover(); return successResponse(command.id, { hovered: true });
  }
  return successResponse(command.id);
}

async function handleGetByText(command: GetByTextCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByText(command.text, { exact: command.exact });
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "hover": await locator.hover(); return successResponse(command.id, { hovered: true });
  }
  return successResponse(command.id);
}

async function handleGetByLabel(command: GetByLabelCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByLabel(command.label, { exact: command.exact });
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "fill": await locator.fill(command.value ?? ""); return successResponse(command.id, { filled: true });
    case "check": await locator.check(); return successResponse(command.id, { checked: true });
  }
  return successResponse(command.id);
}

async function handleGetByPlaceholder(command: GetByPlaceholderCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByPlaceholder(command.placeholder, { exact: command.exact });
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "fill": await locator.fill(command.value ?? ""); return successResponse(command.id, { filled: true });
  }
  return successResponse(command.id);
}

async function handleGetByAltText(command: GetByAltTextCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByAltText(command.text, { exact: command.exact });
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "hover": await locator.hover(); return successResponse(command.id, { hovered: true });
  }
  return successResponse(command.id);
}

async function handleGetByTitle(command: GetByTitleCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByTitle(command.text, { exact: command.exact });
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "hover": await locator.hover(); return successResponse(command.id, { hovered: true });
  }
  return successResponse(command.id);
}

async function handleGetByTestId(command: GetByTestIdCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = page.getByTestId(command.testId);
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "fill": await locator.fill(command.value ?? ""); return successResponse(command.id, { filled: true });
    case "check": await locator.check(); return successResponse(command.id, { checked: true });
    case "hover": await locator.hover(); return successResponse(command.id, { hovered: true });
  }
  return successResponse(command.id);
}

async function handleNth(command: NthCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const base = page.locator(command.selector);
  const locator = command.index === -1 ? base.last() : base.nth(command.index);
  switch (command.subaction) {
    case "click": await locator.click(); return successResponse(command.id, { clicked: true });
    case "fill": await locator.fill(command.value ?? ""); return successResponse(command.id, { filled: true });
    case "check": await locator.check(); return successResponse(command.id, { checked: true });
    case "hover": await locator.hover(); return successResponse(command.id, { hovered: true });
    case "text": { const text = await locator.textContent(); return successResponse(command.id, { text }); }
  }
  return successResponse(command.id);
}

// --- Content / evaluation ---

async function handleSnapshot(command: SnapshotCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const { tree, refs } = await browser.getSnapshot({
    interactive: command.interactive,
    cursor: command.cursor,
    maxDepth: command.maxDepth,
    compact: command.compact,
    selector: command.selector,
  });
  const simpleRefs: Record<string, { role: string; name?: string }> = {};
  for (const [ref, data] of Object.entries(refs)) {
    simpleRefs[ref] = { role: data.role, name: data.name };
  }
  return successResponse(command.id, {
    snapshot: tree || "Empty page",
    refs: Object.keys(simpleRefs).length > 0 ? simpleRefs : undefined,
  });
}

async function handleEvaluate(command: EvaluateCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.evaluate(command.script);
  return successResponse(command.id, { result });
}

async function handleEvalHandle(command: EvalHandleCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.evaluateHandle(command.script);
  return successResponse(command.id, { result });
}

async function handleContent(command: ContentCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const html = await browser.content(command.selector);
  return successResponse(command.id, { html });
}

async function handleSetContent(command: SetContentCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setContent(command.html);
  return successResponse(command.id, { set: true });
}

async function handleScreenshot(command: ScreenshotCommand, browser: BrowserManager): Promise<ResponsePayload> {
  try {
    const filePath = await browser.screenshot({
      path: command.path, fullPage: command.fullPage, selector: command.selector, format: command.format, quality: command.quality,
    });
    return successResponse(command.id, { path: filePath });
  } catch (error) {
    if (command.selector) throw toAIFriendlyError(error, command.selector);
    throw error;
  }
}

async function handlePdf(command: PdfCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.pdf(command.path, command.format);
  return successResponse(command.id, { path: command.path });
}

// --- Wait ---

async function handleWait(command: WaitCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.wait({ selector: command.selector, timeout: command.timeout, state: command.state });
  return successResponse(command.id, { waited: true });
}

async function handleWaitForUrl(command: WaitForUrlCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.waitForUrl(command.url, command.timeout);
  const page = browser.getPage();
  return successResponse(command.id, { url: page.url() });
}

async function handleWaitForLoadState(command: WaitForLoadStateCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.waitForLoadState(command.state, command.timeout);
  return successResponse(command.id, { state: command.state });
}

async function handleWaitForFunction(command: WaitForFunctionCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.waitForFunction(command.expression, command.timeout);
  return successResponse(command.id, { waited: true });
}

async function handleWaitForDownload(command: WaitForDownloadCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const download = await page.waitForEvent("download", { timeout: command.timeout });
  let filePath: string;
  if (command.path) {
    filePath = command.path;
    await download.saveAs(filePath);
  } else {
    filePath = (await download.path()) || download.suggestedFilename();
  }
  return successResponse(command.id, { path: filePath, filename: download.suggestedFilename(), url: download.url() });
}

// --- Element queries ---

async function handleGetAttribute(command: GetAttributeCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const value = await browser.getAttribute(command.selector, command.attribute);
  return successResponse(command.id, { attribute: command.attribute, value });
}

async function handleGetText(command: GetTextCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const text = await browser.getText(command.selector);
  return successResponse(command.id, { text });
}

async function handleInnerText(command: InnerTextCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const text = await browser.innerText(command.selector);
  return successResponse(command.id, { text });
}

async function handleInnerHtml(command: InnerHtmlCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const html = await browser.innerHTML(command.selector);
  return successResponse(command.id, { html });
}

async function handleInputValue(command: InputValueCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const value = await browser.inputValue(command.selector);
  return successResponse(command.id, { value });
}

async function handleSetValue(command: SetValueCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.fill(command.selector, command.value);
  return successResponse(command.id, { set: true });
}

async function handleIsVisible(command: IsVisibleCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const visible = await browser.isVisible(command.selector);
  return successResponse(command.id, { visible });
}

async function handleIsEnabled(command: IsEnabledCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const enabled = await browser.isEnabled(command.selector);
  return successResponse(command.id, { enabled });
}

async function handleIsChecked(command: IsCheckedCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const checked = await browser.isChecked(command.selector);
  return successResponse(command.id, { checked });
}

async function handleCount(command: CountCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const count = await browser.count(command.selector);
  return successResponse(command.id, { count });
}

async function handleBoundingBox(command: BoundingBoxCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const box = await browser.boundingBox(command.selector);
  return successResponse(command.id, { box });
}

async function handleStyles(command: StylesCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const elements = await browser.getStyles(command.selector);
  return successResponse(command.id, { elements });
}

async function handleDispatch(command: DispatchEventCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.dispatchEvent(command.selector, command.event, command.eventInit);
  return successResponse(command.id, { dispatched: command.event });
}

// --- Viewport ---

async function handleViewport(command: ViewportCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setViewport(command.width, command.height);
  return successResponse(command.id, { width: command.width, height: command.height });
}

// --- Navigation (simple) ---

async function handleBack(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.back();
  return successResponse(command.id, { url: browser.getPage().url() });
}

async function handleForward(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.forward();
  return successResponse(command.id, { url: browser.getPage().url() });
}

async function handleReload(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.reload();
  return successResponse(command.id, { url: browser.getPage().url() });
}

async function handleClose(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.close();
  return successResponse(command.id, { closed: true });
}

async function handleUrl(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  return successResponse(command.id, { url: browser.getPage().url() });
}

async function handleTitle(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  const title = await browser.getTitle();
  return successResponse(command.id, { title });
}

// --- Tabs ---

async function handleTabNew(command: TabNewCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.newTab();
  if (command.url) {
    const page = browser.getPage();
    await page.goto(command.url, { waitUntil: "domcontentloaded" });
  }
  return successResponse(command.id, result);
}

async function handleTabList(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  const tabs = await browser.listTabs();
  return successResponse(command.id, { tabs, active: browser.getActiveIndex() });
}

async function handleTabSwitch(command: TabSwitchCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.switchTo(command.index);
  const page = browser.getPage();
  return successResponse(command.id, { ...result, title: await page.title() });
}

async function handleTabClose(command: TabCloseCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.closeTab(command.index);
  return successResponse(command.id, result);
}

async function handleWindowNew(command: WindowNewCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.newWindow(command.viewport);
  return successResponse(command.id, result);
}

// --- Cookies ---

async function handleCookiesGet(command: Command & { urls?: string[] }, browser: BrowserManager): Promise<ResponsePayload> {
  const cookies = await browser.cookiesGet(command.urls);
  return successResponse(command.id, { cookies });
}

async function handleCookiesSet(command: CookiesSetCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.cookiesSet(command.cookies as Parameters<BrowserManager["cookiesSet"]>[0]);
  return successResponse(command.id, { set: true });
}

async function handleCookiesClear(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.cookiesClear();
  return successResponse(command.id, { cleared: true });
}

// --- Storage ---

async function handleStorageGet(command: StorageGetCommand, browser: BrowserManager): Promise<ResponsePayload> {
  if (command.key) {
    const value = await browser.storageGet(command.type, command.key);
    return successResponse(command.id, { key: command.key, value });
  }
  const data = await browser.storageGet(command.type);
  return successResponse(command.id, { data });
}

async function handleStorageSet(command: StorageSetCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.storageSet(command.type, command.key, command.value);
  return successResponse(command.id, { set: true });
}

async function handleStorageClear(command: StorageClearCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.storageClear(command.type);
  return successResponse(command.id, { cleared: true });
}

async function handleStorageState(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  const storageState = await browser.storageState();
  return successResponse(command.id, { storageState });
}

async function handleStorageStateSet(command: Command & { storageState?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  if (!command.storageState) throw new Error("storage_state_set requires storageState");
  await browser.applyStorageState(command.storageState);
  return successResponse(command.id);
}

async function handleStateSave(command: StorageStateSaveCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.saveStorageState(command.path);
  return successResponse(command.id, { path: command.path });
}

async function handleStateLoad(command: Command & { path?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  return successResponse(command.id, {
    note: "Storage state must be loaded at browser launch. Use --state flag.",
    path: command.path,
  });
}

// --- Dialog ---

async function handleDialog(command: DialogCommand, browser: BrowserManager): Promise<ResponsePayload> {
  browser.setDialogHandler(command.response, command.promptText);
  return successResponse(command.id, { handler: "set", response: command.response });
}

// --- Network ---

async function handleRoute(command: RouteCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.addRoute(command.url, { response: command.response, abort: command.abort });
  return successResponse(command.id, { routed: command.url });
}

async function handleUnroute(command: Command & { url?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.removeRoute(command.url);
  return successResponse(command.id, { unrouted: command.url ?? "all" });
}

async function handleRequests(command: RequestsCommand, browser: BrowserManager): Promise<ResponsePayload> {
  if (command.clear) {
    browser.clearRequests();
    return successResponse(command.id, { cleared: true });
  }
  browser.startRequestTracking();
  const requests = browser.getRequests(command.filter);
  return successResponse(command.id, { requests });
}

async function handleDownload(command: DownloadCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const locator = browser.getLocator(command.selector);
  const [download] = await Promise.all([page.waitForEvent("download"), locator.click()]);
  await download.saveAs(command.path);
  return successResponse(command.id, { path: command.path, suggestedFilename: download.suggestedFilename() });
}

async function handleResponseBody(command: ResponseBodyCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const response = await page.waitForResponse((resp) => resp.url().includes(command.url), { timeout: command.timeout });
  const body = await response.text();
  let parsed: unknown = body;
  try { parsed = JSON.parse(body); } catch { /* keep as string */ }
  return successResponse(command.id, { url: response.url(), status: response.status(), body: parsed });
}

async function handleHeaders(command: HeadersCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setExtraHeaders(command.headers);
  return successResponse(command.id, { set: true });
}

// --- Emulation ---

async function handleGeolocation(command: GeolocationCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setGeolocation(command.latitude, command.longitude, command.accuracy);
  return successResponse(command.id, { latitude: command.latitude, longitude: command.longitude });
}

async function handlePermissions(command: PermissionsCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setPermissions(command.permissions, command.grant);
  return successResponse(command.id, { permissions: command.permissions, granted: command.grant });
}

async function handleDevice(command: DeviceCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const device = browser.getDevice(command.device);
  if (!device) {
    const available = browser.listDevices().slice(0, 10).join(", ");
    throw new Error(`Unknown device: ${command.device}. Available: ${available}...`);
  }
  await browser.setViewport(device.viewport.width, device.viewport.height);
  if (device.deviceScaleFactor && device.deviceScaleFactor !== 1) {
    await browser.setDeviceScaleFactor(device.deviceScaleFactor, device.viewport.width, device.viewport.height, device.isMobile ?? false);
  } else {
    try { await browser.clearDeviceMetricsOverride(); } catch { /* ignore */ }
  }
  return successResponse(command.id, { device: command.device, viewport: device.viewport, userAgent: device.userAgent, deviceScaleFactor: device.deviceScaleFactor });
}

async function handleUserAgent(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  return successResponse(command.id, { note: "User agent can only be set at launch time. Use device command instead." });
}

async function handleEmulateMedia(command: EmulateMediaCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.emulateMedia({ media: command.media, colorScheme: command.colorScheme, reducedMotion: command.reducedMotion, forcedColors: command.forcedColors });
  return successResponse(command.id, { emulated: true });
}

async function handleOffline(command: OfflineCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setOffline(command.offline);
  return successResponse(command.id, { offline: command.offline });
}

async function handleTimezone(command: Command & { timezone?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  return successResponse(command.id, { note: "Timezone must be set at browser launch.", timezone: command.timezone });
}

async function handleLocale(command: Command & { locale?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  return successResponse(command.id, { note: "Locale must be set at browser launch.", locale: command.locale });
}

async function handleCredentials(command: HttpCredentialsCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.setHttpCredentials(command.username, command.password);
  return successResponse(command.id, { set: true });
}

// --- Console / errors ---

async function handleConsole(command: ConsoleCommand, browser: BrowserManager): Promise<ResponsePayload> {
  if (command.clear) {
    browser.clearConsoleMessages();
    return successResponse(command.id, { cleared: true });
  }
  return successResponse(command.id, { messages: browser.getConsoleMessages() });
}

async function handleErrors(command: ErrorsCommand, browser: BrowserManager): Promise<ResponsePayload> {
  if (command.clear) {
    browser.clearPageErrors();
    return successResponse(command.id, { cleared: true });
  }
  return successResponse(command.id, { errors: browser.getPageErrors() });
}

// --- Keyboard / mouse ---

async function handleKeyboard(command: KeyboardCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.keyboardPress(command.keys);
  return successResponse(command.id, { pressed: command.keys });
}

async function handleKeyDown(command: KeyDownCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.keyDown(command.key);
  return successResponse(command.id, { down: true, key: command.key });
}

async function handleKeyUp(command: KeyUpCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.keyUp(command.key);
  return successResponse(command.id, { up: true, key: command.key });
}

async function handleInsertText(command: InsertTextCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.insertText(command.text);
  return successResponse(command.id, { inserted: true });
}

async function handleMouseMove(command: MouseMoveCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.mouseMove(command.x, command.y);
  return successResponse(command.id, { moved: true, x: command.x, y: command.y });
}

async function handleMouseDown(command: MouseDownCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.mouseDown(command.button);
  return successResponse(command.id, { down: true });
}

async function handleMouseUp(command: MouseUpCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.mouseUp(command.button);
  return successResponse(command.id, { up: true });
}

async function handleWheel(command: WheelCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.wheel(command.deltaX ?? 0, command.deltaY ?? 0, command.selector);
  return successResponse(command.id, { scrolled: true });
}

async function handleClipboard(command: ClipboardCommand, browser: BrowserManager): Promise<ResponsePayload> {
  switch (command.operation) {
    case "copy": await browser.clipboardCopy(); return successResponse(command.id, { copied: true });
    case "paste": await browser.clipboardPaste(); return successResponse(command.id, { pasted: true });
    case "read": { const text = await browser.clipboardRead(); return successResponse(command.id, { text }); }
    default: return errorResponse(command.id, "Unknown clipboard operation");
  }
}

async function handleMultiSelect(command: MultiSelectCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const selected = await browser.select(command.selector, command.values);
  return successResponse(command.id, { selected });
}

// --- Script / style injection ---

async function handleAddScript(command: AddScriptCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.addScriptTag({ content: command.content, url: command.url });
  return successResponse(command.id, { added: true });
}

async function handleAddStyle(command: AddStyleCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.addStyleTag({ content: command.content, url: command.url });
  return successResponse(command.id, { added: true });
}

async function handleAddInitScript(command: AddInitScriptCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.addInitScript(command.script);
  return successResponse(command.id, { added: true });
}

async function handleExpose(command: Command & { name?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.exposeFunction(command.name ?? "");
  return successResponse(command.id, { exposed: command.name });
}

// --- Video / recording ---

async function handleVideoStart(command: Command & { path?: string }, browser: BrowserManager): Promise<ResponsePayload> {
  return successResponse(command.id, {
    note: "Video recording must be enabled at browser launch. Use --video flag when starting.",
    path: command.path,
  });
}

async function handleVideoStop(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  const video = page.video();
  if (video) {
    const videoPath = await video.path();
    return successResponse(command.id, { path: videoPath });
  }
  return successResponse(command.id, { note: "No video recording active" });
}

async function handleRecordingStart(command: RecordingStartCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.startRecording(command.path, command.url);
  return successResponse(command.id, { started: true, path: command.path });
}

async function handleRecordingStop(_command: RecordingStopCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.stopRecording();
  return successResponse(_command.id, result);
}

async function handleRecordingRestart(command: RecordingRestartCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const result = await browser.restartRecording(command.path, command.url);
  return successResponse(command.id, { started: true, path: command.path, previousPath: result.previousPath, stopped: result.stopped });
}

// --- Tracing / HAR ---

async function handleTraceStart(command: TraceStartCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.startTracing({ screenshots: command.screenshots, snapshots: command.snapshots });
  return successResponse(command.id, { started: true });
}

async function handleTraceStop(command: TraceStopCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.stopTracing(command.path);
  return successResponse(command.id, { path: command.path });
}

async function handleHarStart(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.startHarRecording();
  browser.startRequestTracking();
  return successResponse(command.id, { started: true });
}

async function handleHarStop(command: HarStopCommand, browser: BrowserManager): Promise<ResponsePayload> {
  const requests = browser.getRequests();
  return successResponse(command.id, { path: command.path, requestCount: requests.length });
}

// --- Screencast ---

async function handleScreencastStart(command: ScreencastStartCommand, browser: BrowserManager): Promise<ResponsePayload> {
  if (!screencastFrameCallback) {
    throw new Error("Screencast frame callback not set. Start the streaming server first.");
  }
  await browser.startScreencast(screencastFrameCallback, {
    format: command.format ?? "jpeg",
    quality: command.quality ?? 80,
    maxWidth: command.maxWidth ?? 1280,
    maxHeight: command.maxHeight ?? 720,
    everyNthFrame: command.everyNthFrame ?? 1,
  });
  return successResponse(command.id, { started: true, format: command.format ?? "jpeg", quality: command.quality ?? 80 });
}

async function handleScreencastStop(command: ScreencastStopCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.stopScreencast();
  return successResponse(command.id, { stopped: true });
}

// --- CDP input injection ---

async function handleInputMouse(command: InputMouseCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.injectMouseEvent({ type: command.type, x: command.x, y: command.y, button: command.button, clickCount: command.clickCount, deltaX: command.deltaX, deltaY: command.deltaY, modifiers: command.modifiers });
  return successResponse(command.id, { injected: true });
}

async function handleInputKeyboard(command: InputKeyboardCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.injectKeyboardEvent({ type: command.type, key: command.key, code: command.code, text: command.text, modifiers: command.modifiers });
  return successResponse(command.id, { injected: true });
}

async function handleInputTouch(command: InputTouchCommand, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.injectTouchEvent({ type: command.type, touchPoints: command.touchPoints, modifiers: command.modifiers });
  return successResponse(command.id, { injected: true });
}

// --- Misc ---

async function handleBringToFront(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  await browser.bringToFront();
  return successResponse(command.id, { focused: true });
}

async function handlePause(command: Command, browser: BrowserManager): Promise<ResponsePayload> {
  const page = browser.getPage();
  await page.pause();
  return successResponse(command.id, { paused: true });
}
