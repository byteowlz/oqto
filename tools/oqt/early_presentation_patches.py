"""Pinned transport adapter hooks, not a fork of Durable or its stores.

The owned presentation driver (presentation-client.ts) mounts this native
component before host preparation. One startup message may be queued; it is
dispatched exactly once, through the normal prompt/steer admission, with the
attached conversation's router token after Session, conversation, model and
host identities are bound. Cancelled, rejected, failed or uncertain text returns
to the editor and is never replayed automatically. Commands wait for readiness.
"""

ADAPTER = '''\tstatic async createPending(options: {
\t\treadonly ui: TUI;
\t\treadonly runtime: string;
\t\treadonly facetLoader?: FacetLoader;
\t\t/** The startup queue created with the first frame; adopted, never replaced. */
\t\treadonly startup: {
\t\t\treadonly queue: ReturnType<typeof createPresentationStartup>;
\t\t\tbind(
\t\t\t\tdispatch: (submission: StartupSubmission) => Promise<"accepted" | "rejected">,
\t\t\t\tchanged: () => void,
\t\t\t): void;
\t\t};
\t\trequestRender(): void;
\t\tfinish(): void;
\t}): Promise<ExperimentalClientTui> {
\t\tconst facets = await combineFacetLoaders(
\t\t\toptions.facetLoader === undefined ? [] : [options.facetLoader],
\t\t).load();
\t\tconst component = new ExperimentalClientTui(options.ui, options.requestRender, options.finish, facets);
\t\tcomponent.#startupPending = true;
\t\tcomponent.#startupRuntime = options.runtime;
\t\tcomponent.#startup = options.startup.queue;
\t\toptions.startup.bind(
\t\t\t(submission) => component.#dispatchStartup(submission),
\t\t\t() => component.#startupChanged(),
\t\t);
\t\treturn component;
\t}

\t/** Take over the shell's editor text during the synchronous handoff. */
\tadoptStartupDraft(text: string): void {
\t\tif (text.length > 0) this.#chatInput.setText(text);
\t\tthis.#startupChanged();
\t}

\t/** Attach the prepared Session, then release at most one queued message to
\t * the exact attached target. Never selects another Session on failure. */
\tasync attachPrepared(command: ClientCommand, servers: readonly ClientTuiServer[], host: string): Promise<void> {
\t\tif (this.#closed || !this.#startupPending) throw new Error("Presentation is not awaiting attachment");
\t\tconst prepared = await prepareClientSession(command, servers);
\t\tif (this.#closed) return;
\t\tawait this.#start(prepared);
\t\tif (this.#closed) return;
\t\tawait this.#openPreparedSession(prepared);
\t\tif (this.#closed) return;
\t\tconst startup = this.#startup;
\t\tconst view = this.#conversationView();
\t\tconst agent = (view?.docs["pi.agent"] ?? {}) as AgentState;
\t\tconst model = agent.model === undefined ? "" : `${agent.model.provider}/${agent.model.modelId}`;
\t\tif (startup !== undefined) {
\t\t\tif (!model) {
\t\t\t\tthis.#startupNotice = "Queued message not sent: no model is configured";
\t\t\t\tstartup.fail();
\t\t\t} else {
\t\t\t\ttry {
\t\t\t\t\tawait startup.ready({
\t\t\t\t\t\truntime: this.#startupRuntime,
\t\t\t\t\t\tsession: this.#sessionId ?? "",
\t\t\t\t\t\tconversation: view === undefined ? "" : String(view.conversation.id),
\t\t\t\t\t\tmodel,
\t\t\t\t\t\thost,
\t\t\t\t\t});
\t\t\t\t} catch (error) {
\t\t\t\t\tthis.#startupNotice = `Queued message not sent: ${message(error)}`;
\t\t\t\t\tstartup.fail();
\t\t\t\t}
\t\t\t}
\t\t}
\t\tif (this.#closed) return;
\t\tthis.#startupPending = false;
\t\tthis.#startupChanged();
\t}

\t/** Startup could not reach a Session: keep the editor and draft, send nothing. */
\tfailStartup(reason: string): void {
\t\tif (this.#closed) return;
\t\tthis.#startupNotice = `Startup failed; nothing was sent: ${reason}`;
\t\tif (this.#startup === undefined) this.#startupChanged();
\t\telse this.#startup.fail();
\t}

\tasync #dispatchStartup(submission: StartupSubmission): Promise<"accepted" | "rejected"> {
\t\tconst view = this.#conversationView();
\t\tconst controller = this.#selectedController();
\t\tif (
\t\t\tthis.#closed ||
\t\t\tview === undefined ||
\t\t\tcontroller === undefined ||
\t\t\tthis.#sessionId !== submission.binding.session ||
\t\t\tString(view.conversation.id) !== submission.binding.conversation
\t\t) {
\t\t\tthis.#startupNotice = "Queued message not sent: the attached conversation changed";
\t\t\treturn "rejected";
\t\t}
\t\t// The router rejects this token if another client selected another branch.
\t\tconst context = captureConversationContext(view, BACKGROUND_CONTEXT);
\t\tconst request = { message: submission.text, images: null };
\t\tconst response =
\t\t\tliveOf(view).run !== undefined
\t\t\t\t? await controller.steer(request, context)
\t\t\t\t: await controller.prompt(request, context);
\t\tif (!response.accepted) this.#startupNotice = `Queued message not accepted: ${response.error.message}`;
\t\treturn response.accepted ? "accepted" : "rejected";
\t}

\t#startupChanged(): void {
\t\tconst startup = this.#startup;
\t\tif (startup === undefined) {
\t\t\tif (this.#startupNotice) this.#status = this.#startupNotice;
\t\t\tthis.#rebuild();
\t\t\treturn;
\t\t}
\t\tconst view = startup.input.view();
\t\tlet restored = false;
\t\tfor (const item of view.retained) {
\t\t\tif (this.#chatInput.getText().length > 0) break;
\t\t\tthis.#chatInput.setText(item.text);
\t\t\tstartup.input.release(item.requestId);
\t\t\trestored = true;
\t\t}
\t\tconst kept = startup.input.view().retained.length;
\t\tconst suffix = restored ? " — text restored to the editor" : kept > 0 ? ` — ${kept} unsent message(s) kept; Esc on an empty editor restores` : "";
\t\tswitch (view.status) {
\t\t\tcase "loading":
\t\t\t\tthis.#status = `Loading oqt — type now; Enter queues one message${suffix}`;
\t\t\t\tbreak;
\t\t\tcase "queued":
\t\t\t\tthis.#status = "Queued — sends when oqt is ready (Esc cancels)";
\t\t\t\tbreak;
\t\t\tcase "sending":
\t\t\t\tthis.#status = "Sending queued message…";
\t\t\t\tbreak;
\t\t\tcase "sent":
\t\t\tcase "idle":
\t\t\t\tif (!this.#startupPending && this.#status.startsWith("Loading oqt")) this.#status = "";
\t\t\t\tif (!this.#startupPending && this.#status.startsWith("Queued —")) this.#status = "";
\t\t\t\tif (!this.#startupPending && this.#status.startsWith("Sending queued")) this.#status = "";
\t\t\t\tbreak;
\t\t\tcase "uncertain":
\t\t\t\tthis.#status = `Queued message may have been sent (connection lost): check the transcript before resending${suffix}`;
\t\t\t\tbreak;
\t\t\tdefault:
\t\t\t\tthis.#status = `${this.#startupNotice || "Queued message not sent"}${suffix}`;
\t\t}
\t\tthis.#rebuild();
\t}

\t#queueStartupInput(text: string): void {
\t\tconst startup = this.#startup;
\t\tconst trimmed = text.trim();
\t\tif (trimmed.length === 0) return;
\t\tif (trimmed.startsWith("/")) {
\t\t\tthis.#chatInput.setText(text);
\t\t\tthis.#status = "Commands are available once oqt is ready — draft kept";
\t\t\tthis.#rebuild();
\t\t\treturn;
\t\t}
\t\tif (startup?.input.submit(text)) return;
\t\tthis.#chatInput.setText(text);
\t\tconst phase = startup?.input.view();
\t\tthis.#status =
\t\t\tphase?.phase === "failed"
\t\t\t\t? "oqt did not start — draft kept; exit and relaunch"
\t\t\t\t: phase?.queued || phase?.sending
\t\t\t\t\t? "A startup message is already queued — draft kept (Esc cancels the queued one)"
\t\t\t\t\t: "Not queued — draft kept";
\t\tthis.#rebuild();
\t}

\tget layoutRoot(): Component {'''


def apply(snapshot):
    path = snapshot / 'packages/coding-agent/src/experimental/client-tui.ts'
    changes = [
        ('import { captureConversationContext } from "./session-router.ts";',
         'import { captureConversationContext } from "./session-router.ts";\n'
         'import type { createPresentationStartup, StartupSubmission } from "./presentation-startup.ts";\n'
         'import { programStatus } from "./program-status.ts";'),
        # OSC 7501: attach once per TUI, observe every view change, clear on exit.
        ('\t\tthis.#ui = ui;\n', '\t\tthis.#ui = ui;\n\t\tprogramStatus.attach(ui.terminal);\n'),
        ('\t#rebuild(): void {\n',
         '\t#rebuild(): void {\n'
         '\t\tconst statusView = this.#conversationView();\n'
         '\t\tif (statusView !== undefined) programStatus.observe(liveOf(statusView).run !== undefined, statusView.entries as never);\n'),
        ('\t#busy = false;',
         '\t#startupPending = false;\n\t#startupRuntime = "";\n\t#startupNotice = "";\n'
         '\t#startup: ReturnType<typeof createPresentationStartup> | undefined;\n\t#busy = false;'),
        ('\t\tif (this.#busy) {', '\t\tif (this.#busy && !this.#startupPending) {'),
        ('\tget layoutRoot(): Component {', ADAPTER),
        ('\tasync #runPrompt(messageText: string): Promise<void> {\n',
         '\tasync #runPrompt(messageText: string): Promise<void> {\n'
         '\t\tif (this.#startupPending) {\n\t\t\tthis.#queueStartupInput(messageText);\n\t\t\treturn;\n\t\t}\n'),
        ('\tasync #queueFollowUp(text: string): Promise<void> {\n',
         '\tasync #queueFollowUp(text: string): Promise<void> {\n'
         '\t\tif (this.#startupPending) {\n\t\t\tthis.#queueStartupInput(text);\n\t\t\treturn;\n\t\t}\n'),
        ('\tasync #executeSlashCommand(name: string, args: string): Promise<void> {\n',
         '\tasync #executeSlashCommand(name: string, args: string): Promise<void> {\n'
         '\t\tif (this.#startupPending) {\n'
         '\t\t\tthis.#status = "Commands are available once oqt is ready";\n'
         '\t\t\tthis.#rebuild();\n\t\t\treturn;\n\t\t}\n'),
        ('\t#interrupt(): void {\n',
         '\t#interrupt(): void {\n'
         '\t\tif (this.#startupPending) {\n'
         '\t\t\tconst startup = this.#startup;\n'
         '\t\t\t// cancel() notifies once; that render restores the text and says so.\n'
         '\t\t\tif (startup?.input.cancel() !== undefined) return;\n'
         '\t\t\tconst next = startup?.input.view().retained[0];\n'
         '\t\t\tif (startup !== undefined && next !== undefined && this.#chatInput.getText().length === 0) {\n'
         '\t\t\t\tthis.#chatInput.setText(next.text);\n'
         '\t\t\t\tstartup.input.release(next.requestId);\n'
         '\t\t\t}\n'
         '\t\t\tthis.#startupChanged();\n'
         '\t\t\treturn;\n\t\t}\n'),
        # Pre-existing race made frequent by a live renderer: a plugin may open a
        # dialog (e.g. an already pending approval) during #start; attaching the
        # lane must not discard it by forcing the chat screen.
        ('\t\tawait this.#openLane(feature);\n\t\tthis.#screen = "chat";\n',
         '\t\tawait this.#openLane(feature);\n\t\tif (this.#selection === undefined) this.#screen = "chat";\n'),
        ('\tasync #close(): Promise<void> {\n\t\tthis.#closed = true;\n',
         '\tasync #close(): Promise<void> {\n\t\tthis.#closed = true;\n\t\tthis.#startup?.close();\n\t\tprogramStatus.detach();\n'),
    ]
    text = path.read_text()
    for old, new in changes:
        if text.count(old) != 1:
            raise SystemExit(f'Upstream changed; refusing early presentation adapter patch: {old[:60]!r}')
        text = text.replace(old, new)
    path.write_text(text)
    return [path]
