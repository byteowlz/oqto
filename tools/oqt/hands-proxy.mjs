// Transport only. This process has no execution implementation or authority.
// The epoch names the hands generation the brain was told to use; the guard
// rejects any other generation, so retained handles cannot reconnect.
import { connect } from "node:net";

const [socketPath, epoch, serve, flag, token] = process.argv.slice(2);
if (
	!/^[a-f0-9]{32}$/.test(epoch ?? "") ||
	serve !== "serve" ||
	flag !== "--token" ||
	!/^[a-f0-9]{32}$/.test(token ?? "")
)
	throw new Error("Invalid confined transport invocation");
const socket = connect(socketPath);
socket.once("connect", () => {
	socket.write(`${JSON.stringify({ token, epoch })}\n`);
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
