// Base command structure
export interface BaseCommand {
  id: string;
  action: string;
}

// Launch
export interface LaunchCommand extends BaseCommand {
  action: "launch";
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

// Navigation
export interface NavigateCommand extends BaseCommand {
  action: "navigate";
  url: string;
  waitUntil?: "load" | "domcontentloaded" | "networkidle";
  headers?: Record<string, string>;
}

// Interaction
export interface ClickCommand extends BaseCommand {
  action: "click";
  selector: string;
  button?: "left" | "right" | "middle";
  clickCount?: number;
  delay?: number;
}

export interface TypeCommand extends BaseCommand {
  action: "type";
  selector: string;
  text: string;
  delay?: number;
  clear?: boolean;
}

export interface FillCommand extends BaseCommand {
  action: "fill";
  selector: string;
  value: string;
}

export interface PressCommand extends BaseCommand {
  action: "press";
  key: string;
  selector?: string;
}

export interface CheckCommand extends BaseCommand {
  action: "check";
  selector: string;
}

export interface UncheckCommand extends BaseCommand {
  action: "uncheck";
  selector: string;
}

export interface UploadCommand extends BaseCommand {
  action: "upload";
  selector: string;
  files: string | string[];
}

export interface DoubleClickCommand extends BaseCommand {
  action: "dblclick";
  selector: string;
}

export interface FocusCommand extends BaseCommand {
  action: "focus";
  selector: string;
}

export interface DragCommand extends BaseCommand {
  action: "drag";
  source: string;
  target: string;
}

export interface HoverCommand extends BaseCommand {
  action: "hover";
  selector: string;
}

export interface SelectCommand extends BaseCommand {
  action: "select";
  selector: string;
  values: string | string[];
}

export interface TapCommand extends BaseCommand {
  action: "tap";
  selector: string;
}

export interface ClearCommand extends BaseCommand {
  action: "clear";
  selector: string;
}

export interface SelectAllCommand extends BaseCommand {
  action: "selectall";
  selector: string;
}

export interface HighlightCommand extends BaseCommand {
  action: "highlight";
  selector: string;
}

export interface ScrollIntoViewCommand extends BaseCommand {
  action: "scrollintoview";
  selector: string;
}

export interface ScrollCommand extends BaseCommand {
  action: "scroll";
  selector?: string;
  x?: number;
  y?: number;
  direction?: "up" | "down" | "left" | "right";
  amount?: number;
}

// Frame
export interface FrameCommand extends BaseCommand {
  action: "frame";
  selector?: string;
  name?: string;
  url?: string;
}

export interface MainFrameCommand extends BaseCommand {
  action: "mainframe";
}

// Locators
export interface GetByRoleCommand extends BaseCommand {
  action: "getbyrole";
  role: string;
  name?: string;
  exact?: boolean;
  subaction: "click" | "fill" | "check" | "hover";
  value?: string;
}

export interface GetByTextCommand extends BaseCommand {
  action: "getbytext";
  text: string;
  exact?: boolean;
  subaction: "click" | "hover";
}

export interface GetByLabelCommand extends BaseCommand {
  action: "getbylabel";
  label: string;
  exact?: boolean;
  subaction: "click" | "fill" | "check";
  value?: string;
}

export interface GetByPlaceholderCommand extends BaseCommand {
  action: "getbyplaceholder";
  placeholder: string;
  exact?: boolean;
  subaction: "click" | "fill";
  value?: string;
}

export interface GetByAltTextCommand extends BaseCommand {
  action: "getbyalttext";
  text: string;
  exact?: boolean;
  subaction: "click" | "hover";
}

export interface GetByTitleCommand extends BaseCommand {
  action: "getbytitle";
  text: string;
  exact?: boolean;
  subaction: "click" | "hover";
}

export interface GetByTestIdCommand extends BaseCommand {
  action: "getbytestid";
  testId: string;
  subaction: "click" | "fill" | "check" | "hover";
  value?: string;
}

export interface NthCommand extends BaseCommand {
  action: "nth";
  selector: string;
  index: number;
  subaction: "click" | "fill" | "check" | "hover" | "text";
  value?: string;
}

// Content / evaluation
export interface SnapshotCommand extends BaseCommand {
  action: "snapshot";
  interactive?: boolean;
  cursor?: boolean;
  maxDepth?: number;
  compact?: boolean;
  selector?: string;
}

export interface EvaluateCommand extends BaseCommand {
  action: "evaluate";
  script: string;
}

export interface EvalHandleCommand extends BaseCommand {
  action: "evalhandle";
  script: string;
}

export interface ContentCommand extends BaseCommand {
  action: "content";
  selector?: string;
}

export interface SetContentCommand extends BaseCommand {
  action: "setcontent";
  html: string;
}

export interface ScreenshotCommand extends BaseCommand {
  action: "screenshot";
  path?: string;
  fullPage?: boolean;
  selector?: string;
  format?: "png" | "jpeg";
  quality?: number;
}

export interface PdfCommand extends BaseCommand {
  action: "pdf";
  path: string;
  format?: string;
}

// Wait
export interface WaitCommand extends BaseCommand {
  action: "wait";
  selector?: string;
  timeout?: number;
  state?: "attached" | "detached" | "visible" | "hidden";
}

export interface WaitForUrlCommand extends BaseCommand {
  action: "waitforurl";
  url: string;
  timeout?: number;
}

export interface WaitForLoadStateCommand extends BaseCommand {
  action: "waitforloadstate";
  state: "load" | "domcontentloaded" | "networkidle";
  timeout?: number;
}

export interface WaitForFunctionCommand extends BaseCommand {
  action: "waitforfunction";
  expression: string;
  timeout?: number;
}

export interface WaitForDownloadCommand extends BaseCommand {
  action: "waitfordownload";
  path?: string;
  timeout?: number;
}

// Element queries
export interface GetAttributeCommand extends BaseCommand {
  action: "getattribute";
  selector: string;
  attribute: string;
}

export interface GetTextCommand extends BaseCommand {
  action: "gettext";
  selector: string;
}

export interface InnerTextCommand extends BaseCommand {
  action: "innertext";
  selector: string;
}

export interface InnerHtmlCommand extends BaseCommand {
  action: "innerhtml";
  selector: string;
}

export interface InputValueCommand extends BaseCommand {
  action: "inputvalue";
  selector: string;
}

export interface SetValueCommand extends BaseCommand {
  action: "setvalue";
  selector: string;
  value: string;
}

export interface IsVisibleCommand extends BaseCommand {
  action: "isvisible";
  selector: string;
}

export interface IsEnabledCommand extends BaseCommand {
  action: "isenabled";
  selector: string;
}

export interface IsCheckedCommand extends BaseCommand {
  action: "ischecked";
  selector: string;
}

export interface CountCommand extends BaseCommand {
  action: "count";
  selector: string;
}

export interface BoundingBoxCommand extends BaseCommand {
  action: "boundingbox";
  selector: string;
}

export interface StylesCommand extends BaseCommand {
  action: "styles";
  selector: string;
}

export interface DispatchEventCommand extends BaseCommand {
  action: "dispatch";
  selector: string;
  event: string;
  eventInit?: Record<string, unknown>;
}

// Viewport
export interface ViewportCommand extends BaseCommand {
  action: "viewport";
  width: number;
  height: number;
}

// Simple actions (no extra fields)
export interface SimpleCommand extends BaseCommand {
  action:
    | "back"
    | "forward"
    | "reload"
    | "close"
    | "url"
    | "title"
    | "mainframe"
    | "tab_list"
    | "bringtofront"
    | "pause";
}

// Tabs
export interface TabNewCommand extends BaseCommand {
  action: "tab_new";
  url?: string;
}

export interface TabSwitchCommand extends BaseCommand {
  action: "tab_switch";
  index: number;
}

export interface TabCloseCommand extends BaseCommand {
  action: "tab_close";
  index?: number;
}

export interface WindowNewCommand extends BaseCommand {
  action: "window_new";
  viewport?: { width: number; height: number };
}

// Cookies
export interface CookiesGetCommand extends BaseCommand {
  action: "cookies_get";
  urls?: string[];
}

export interface CookiesSetCommand extends BaseCommand {
  action: "cookies_set";
  cookies: Array<{
    name: string;
    value: string;
    url?: string;
    domain?: string;
    path?: string;
    expires?: number;
    httpOnly?: boolean;
    secure?: boolean;
    sameSite?: "Strict" | "Lax" | "None";
  }>;
}

export interface CookiesClearCommand extends BaseCommand {
  action: "cookies_clear";
}

// localStorage / sessionStorage
export interface StorageGetCommand extends BaseCommand {
  action: "storage_get";
  key?: string;
  type: "local" | "session";
}

export interface StorageSetCommand extends BaseCommand {
  action: "storage_set";
  key: string;
  value: string;
  type: "local" | "session";
}

export interface StorageClearCommand extends BaseCommand {
  action: "storage_clear";
  type: "local" | "session";
}

// Storage state (auth persistence)
export interface StorageStateSaveCommand extends BaseCommand {
  action: "state_save";
  path: string;
}

export interface StorageStateLoadCommand extends BaseCommand {
  action: "state_load";
  path: string;
}

export interface StorageStateCommand extends BaseCommand {
  action: "storage_state";
}

export interface StorageStateSetCommand extends BaseCommand {
  action: "storage_state_set";
  storageState: string;
}

// Dialog
export interface DialogCommand extends BaseCommand {
  action: "dialog";
  response: "accept" | "dismiss";
  promptText?: string;
}

// Network
export interface RouteCommand extends BaseCommand {
  action: "route";
  url: string;
  response?: { status?: number; body?: string; contentType?: string; headers?: Record<string, string> };
  abort?: boolean;
}

export interface UnrouteCommand extends BaseCommand {
  action: "unroute";
  url?: string;
}

export interface RequestsCommand extends BaseCommand {
  action: "requests";
  filter?: string;
  clear?: boolean;
}

export interface DownloadCommand extends BaseCommand {
  action: "download";
  selector: string;
  path: string;
}

export interface ResponseBodyCommand extends BaseCommand {
  action: "responsebody";
  url: string;
  timeout?: number;
}

export interface HeadersCommand extends BaseCommand {
  action: "headers";
  headers: Record<string, string>;
}

// Emulation
export interface GeolocationCommand extends BaseCommand {
  action: "geolocation";
  latitude: number;
  longitude: number;
  accuracy?: number;
}

export interface PermissionsCommand extends BaseCommand {
  action: "permissions";
  permissions: string[];
  grant: boolean;
}

export interface DeviceCommand extends BaseCommand {
  action: "device";
  device: string;
}

export interface EmulateMediaCommand extends BaseCommand {
  action: "emulatemedia";
  media?: "screen" | "print" | null;
  colorScheme?: "light" | "dark" | "no-preference" | null;
  reducedMotion?: "reduce" | "no-preference" | null;
  forcedColors?: "active" | "none" | null;
}

export interface OfflineCommand extends BaseCommand {
  action: "offline";
  offline: boolean;
}

export interface TimezoneCommand extends BaseCommand {
  action: "timezone";
  timezone: string;
}

export interface LocaleCommand extends BaseCommand {
  action: "locale";
  locale: string;
}

export interface HttpCredentialsCommand extends BaseCommand {
  action: "credentials";
  username: string;
  password: string;
}

// Console / errors
export interface ConsoleCommand extends BaseCommand {
  action: "console";
  clear?: boolean;
}

export interface ErrorsCommand extends BaseCommand {
  action: "errors";
  clear?: boolean;
}

// Keyboard / mouse
export interface KeyboardCommand extends BaseCommand {
  action: "keyboard";
  keys: string;
}

export interface KeyDownCommand extends BaseCommand {
  action: "keydown";
  key: string;
}

export interface KeyUpCommand extends BaseCommand {
  action: "keyup";
  key: string;
}

export interface InsertTextCommand extends BaseCommand {
  action: "inserttext";
  text: string;
}

export interface MouseMoveCommand extends BaseCommand {
  action: "mousemove";
  x: number;
  y: number;
}

export interface MouseDownCommand extends BaseCommand {
  action: "mousedown";
  button?: "left" | "right" | "middle";
}

export interface MouseUpCommand extends BaseCommand {
  action: "mouseup";
  button?: "left" | "right" | "middle";
}

export interface WheelCommand extends BaseCommand {
  action: "wheel";
  deltaX?: number;
  deltaY?: number;
  selector?: string;
}

// Clipboard
export interface ClipboardCommand extends BaseCommand {
  action: "clipboard";
  operation: "copy" | "paste" | "read";
}

// Multi-select
export interface MultiSelectCommand extends BaseCommand {
  action: "multiselect";
  selector: string;
  values: string[];
}

// Script / style injection
export interface AddScriptCommand extends BaseCommand {
  action: "addscript";
  content?: string;
  url?: string;
}

export interface AddStyleCommand extends BaseCommand {
  action: "addstyle";
  content?: string;
  url?: string;
}

export interface AddInitScriptCommand extends BaseCommand {
  action: "addinitscript";
  script: string;
}

export interface ExposeFunctionCommand extends BaseCommand {
  action: "expose";
  name: string;
}

// Video
export interface VideoStartCommand extends BaseCommand {
  action: "video_start";
  path: string;
}

export interface VideoStopCommand extends BaseCommand {
  action: "video_stop";
}

// Recording
export interface RecordingStartCommand extends BaseCommand {
  action: "recording_start";
  path: string;
  url?: string;
}

export interface RecordingStopCommand extends BaseCommand {
  action: "recording_stop";
}

export interface RecordingRestartCommand extends BaseCommand {
  action: "recording_restart";
  path: string;
  url?: string;
}

// Tracing
export interface TraceStartCommand extends BaseCommand {
  action: "trace_start";
  screenshots?: boolean;
  snapshots?: boolean;
}

export interface TraceStopCommand extends BaseCommand {
  action: "trace_stop";
  path: string;
}

// HAR
export interface HarStartCommand extends BaseCommand {
  action: "har_start";
}

export interface HarStopCommand extends BaseCommand {
  action: "har_stop";
  path: string;
}

// Screencast
export interface ScreencastStartCommand extends BaseCommand {
  action: "screencast_start";
  format?: "jpeg" | "png";
  quality?: number;
  maxWidth?: number;
  maxHeight?: number;
  everyNthFrame?: number;
}

export interface ScreencastStopCommand extends BaseCommand {
  action: "screencast_stop";
}

// CDP input injection
export interface InputMouseCommand extends BaseCommand {
  action: "input_mouse";
  type: "mousePressed" | "mouseReleased" | "mouseMoved" | "mouseWheel";
  x: number;
  y: number;
  button?: "left" | "right" | "middle" | "none";
  clickCount?: number;
  deltaX?: number;
  deltaY?: number;
  modifiers?: number;
}

export interface InputKeyboardCommand extends BaseCommand {
  action: "input_keyboard";
  type: "keyDown" | "keyUp" | "char";
  key?: string;
  code?: string;
  text?: string;
  modifiers?: number;
}

export interface InputTouchCommand extends BaseCommand {
  action: "input_touch";
  type: "touchStart" | "touchEnd" | "touchMove" | "touchCancel";
  touchPoints: Array<{ x: number; y: number; id?: number }>;
  modifiers?: number;
}

// User agent
export interface UserAgentCommand extends BaseCommand {
  action: "useragent";
  userAgent: string;
}

// Union of all commands
export type Command =
  | LaunchCommand
  | NavigateCommand
  | ClickCommand
  | TypeCommand
  | FillCommand
  | PressCommand
  | CheckCommand
  | UncheckCommand
  | UploadCommand
  | DoubleClickCommand
  | FocusCommand
  | DragCommand
  | HoverCommand
  | SelectCommand
  | TapCommand
  | ClearCommand
  | SelectAllCommand
  | HighlightCommand
  | ScrollIntoViewCommand
  | ScrollCommand
  | FrameCommand
  | MainFrameCommand
  | GetByRoleCommand
  | GetByTextCommand
  | GetByLabelCommand
  | GetByPlaceholderCommand
  | GetByAltTextCommand
  | GetByTitleCommand
  | GetByTestIdCommand
  | NthCommand
  | SnapshotCommand
  | EvaluateCommand
  | ContentCommand
  | SetContentCommand
  | ScreenshotCommand
  | PdfCommand
  | WaitCommand
  | WaitForUrlCommand
  | WaitForLoadStateCommand
  | WaitForFunctionCommand
  | WaitForDownloadCommand
  | GetAttributeCommand
  | GetTextCommand
  | InnerTextCommand
  | InnerHtmlCommand
  | InputValueCommand
  | SetValueCommand
  | IsVisibleCommand
  | IsEnabledCommand
  | IsCheckedCommand
  | CountCommand
  | BoundingBoxCommand
  | StylesCommand
  | DispatchEventCommand
  | ViewportCommand
  | SimpleCommand
  | TabNewCommand
  | TabSwitchCommand
  | TabCloseCommand
  | WindowNewCommand
  | CookiesGetCommand
  | CookiesSetCommand
  | CookiesClearCommand
  | StorageGetCommand
  | StorageSetCommand
  | StorageClearCommand
  | StorageStateSaveCommand
  | StorageStateLoadCommand
  | StorageStateCommand
  | StorageStateSetCommand
  | DialogCommand
  | RouteCommand
  | UnrouteCommand
  | RequestsCommand
  | DownloadCommand
  | ResponseBodyCommand
  | HeadersCommand
  | GeolocationCommand
  | PermissionsCommand
  | DeviceCommand
  | EmulateMediaCommand
  | OfflineCommand
  | TimezoneCommand
  | LocaleCommand
  | HttpCredentialsCommand
  | ConsoleCommand
  | ErrorsCommand
  | KeyboardCommand
  | KeyDownCommand
  | KeyUpCommand
  | InsertTextCommand
  | MouseMoveCommand
  | MouseDownCommand
  | MouseUpCommand
  | WheelCommand
  | ClipboardCommand
  | MultiSelectCommand
  | AddScriptCommand
  | AddStyleCommand
  | AddInitScriptCommand
  | ExposeFunctionCommand
  | VideoStartCommand
  | VideoStopCommand
  | RecordingStartCommand
  | RecordingStopCommand
  | RecordingRestartCommand
  | TraceStartCommand
  | TraceStopCommand
  | HarStartCommand
  | HarStopCommand
  | ScreencastStartCommand
  | ScreencastStopCommand
  | InputMouseCommand
  | InputKeyboardCommand
  | InputTouchCommand
  | UserAgentCommand
  | EvalHandleCommand;

// --- Response types ---

export interface ResponsePayload {
  id: string;
  success: boolean;
  data?: unknown;
  error?: string | null;
}

export function successResponse(id: string, data?: unknown): ResponsePayload {
  return { id, success: true, data: data ?? null };
}

export function errorResponse(id: string, message: string): ResponsePayload {
  return { id, success: false, error: message };
}

// --- Command parsing ---

const KNOWN_ACTIONS = new Set([
  "launch", "navigate", "click", "type", "fill", "press",
  "check", "uncheck", "upload", "dblclick", "focus", "drag",
  "hover", "select", "tap", "clear", "selectall", "highlight",
  "scrollintoview", "scroll", "frame", "mainframe",
  "getbyrole", "getbytext", "getbylabel", "getbyplaceholder",
  "getbyalttext", "getbytitle", "getbytestid", "nth",
  "snapshot", "evaluate", "content", "setcontent", "screenshot", "pdf",
  "wait", "waitforurl", "waitforloadstate", "waitforfunction", "waitfordownload",
  "getattribute", "gettext", "innertext", "innerhtml", "inputvalue", "setvalue",
  "isvisible", "isenabled", "ischecked", "count", "boundingbox", "styles", "dispatch",
  "viewport", "back", "forward", "reload", "close", "url", "title",
  "tab_new", "tab_list", "tab_switch", "tab_close", "window_new",
  "cookies_get", "cookies_set", "cookies_clear",
  "storage_get", "storage_set", "storage_clear",
  "state_save", "state_load", "storage_state", "storage_state_set",
  "dialog", "route", "unroute", "requests", "download", "responsebody", "headers",
  "geolocation", "permissions", "device", "emulatemedia", "offline",
  "timezone", "locale", "credentials", "useragent",
  "console", "errors",
  "keyboard", "keydown", "keyup", "inserttext",
  "mousemove", "mousedown", "mouseup", "wheel",
  "clipboard", "multiselect",
  "addscript", "addstyle", "addinitscript", "expose",
  "video_start", "video_stop",
  "recording_start", "recording_stop", "recording_restart",
  "trace_start", "trace_stop", "har_start", "har_stop",
  "screencast_start", "screencast_stop",
  "input_mouse", "input_keyboard", "input_touch",
  "bringtofront", "pause",
  "evalhandle",
]);

export type CommandParseResult =
  | { success: true; command: Command }
  | { success: false; error: string; id?: string };

export function parseCommand(line: string): CommandParseResult {
  let raw: unknown;
  try {
    raw = JSON.parse(line);
  } catch {
    return { success: false, error: "Invalid JSON" };
  }

  if (!raw || typeof raw !== "object") {
    return { success: false, error: "Invalid command" };
  }

  const data = raw as Record<string, unknown>;
  const id = typeof data.id === "string" ? data.id : undefined;
  const action = typeof data.action === "string" ? data.action : undefined;

  if (!id || !action) {
    return { success: false, error: "Missing id or action", id };
  }

  if (!KNOWN_ACTIONS.has(action)) {
    return { success: false, error: `Unsupported action: ${action}`, id };
  }

  return { success: true, command: data as unknown as Command };
}
