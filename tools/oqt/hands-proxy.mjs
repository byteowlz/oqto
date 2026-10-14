// Transport only. This process has no execution implementation or authority.
import { connect } from "node:net";

const [socketPath, serve, flag, token] = process.argv.slice(2);
if (
	serve !== "serve" ||
	flag !== "--token" ||
	!/^[a-f0-9]{32}$/.test(token ?? "")
)
	throw new Error("Invalid confined transport invocation");
const socket = connect(socketPath);
socket.once("connect", () => {
	socket.write(`${JSON.stringify({ token })}\n`);
	process.stdin.pipe(socket);
	socket.pipe(process.stdout);
});
socket.on("error", () => {
	process.exitCode = 1;
	socket.destroy();
});
socket.once("close", () => {
	process.stdin.destroy();
});
process.stdin.once("end", () => socket.end());
