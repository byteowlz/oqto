/** Local oqt client host: connects to the server's private socket. Objects it
 * gets are placed by their contracts (Session workers), relayed by the server.
 */
import { createHost, type Host, webSocketLink } from "@earendil-works/chord";
import { WebSocket } from "ws";
import { type Principal, principalKey } from "./contract.ts";

export interface OqtClient {
	readonly host: Host<Principal>;
	close(): Promise<void>;
}

export async function connectClient(
	socketPath: string,
	token: string,
): Promise<OqtClient> {
	const socket = new WebSocket(`ws+unix://${socketPath}:/`, {
		headers: { authorization: `Bearer ${token}` },
		maxPayload: 16 * 1024 * 1024,
	});
	// Attach the link before the socket opens: the server's hello can arrive
	// with the open event, and a frame without a listener is lost.
	const host = createHost<Principal>({
		kind: "client",
		self: { kind: "host" },
		principalKey,
		onError: (error) => console.error("[oqt client]", error),
		upstream: {
			link: webSocketLink(socket as never),
			principal: { kind: "server" },
		},
	});
	await new Promise<void>((resolve, reject) => {
		socket.once("open", () => resolve());
		socket.once("error", (error) => {
			void host.close();
			reject(error);
		});
	});
	return {
		host,
		async close() {
			await host.close();
			socket.close();
		},
	};
}
