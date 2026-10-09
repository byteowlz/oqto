import { isAbsolute, join } from "node:path";

/** Load the owning runner's Pi SDK. No sessions, extensions, or hand-written auth files. */
export async function createPiLoginRuntime(sdk, agentDir) {
	if (!isAbsolute(agentDir) || typeof sdk.ModelRuntime?.create !== "function") {
		throw new Error(
			"Managed Pi lacks provider-auth SDK support; use native /login or upgrade its managed runtime",
		);
	}
	const runtime = await sdk.ModelRuntime.create({
		authPath: join(agentDir, "auth.json"),
		modelsPath: join(agentDir, "models.json"),
		modelsStorePath: join(agentDir, "models-store.json"),
		allowModelNetwork: false,
	});
	return {
		async providers() {
			const credentials = await runtime.listCredentials();
			return runtime.getProviders().map((provider) => ({
				id: provider.id,
				name: provider.name,
				methods: [
					provider.auth.apiKey?.login ? "api_key" : null,
					provider.auth.oauth?.login ? "oauth" : null,
				].filter(Boolean),
				configured: credentials.some(
					(entry) => entry.providerId === provider.id,
				),
			}));
		},
		async login(id, type, interaction, beforeCommit) {
			const provider = runtime.getProvider(id);
			const field = type === "oauth" ? "oauth" : "apiKey";
			const method = provider?.auth[field];
			if (!method?.login) throw new Error("Provider login unavailable");
			// Fence revocation/expiry after provider interaction but BEFORE Pi stores credentials.
			runtime.registerNativeProvider({
				...provider,
				auth: {
					...provider.auth,
					[field]: {
						...method,
						login: async (callbacks) => {
							const credential = await method.login(callbacks);
							await beforeCommit();
							callbacks.signal.throwIfAborted();
							return credential;
						},
					},
				},
			});
			try {
				await runtime.login(id, type, {
					...interaction,
					prompt: async (prompt) => {
						const answer = await interaction.prompt(prompt);
						// Pi credential values can execute !commands or resolve $ENV.
						// Browser login grants literal credentials, not host execution.
						if (
							type === "api_key" &&
							(answer.trimStart().startsWith("!") || answer.includes("$"))
						) {
							throw new Error(
								"Credential references require separate managed provisioning",
							);
						}
						return answer;
					},
				});
			} finally {
				runtime.registerNativeProvider(provider);
			}
		},
		credentialsCommitted(error) {
			return (
				typeof sdk.CredentialSynchronizationError === "function" &&
				error instanceof sdk.CredentialSynchronizationError
			);
		},
	};
}
