import { type Plugin, tool } from "@opencode-ai/plugin";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const SERVICE_NAME = "main-delegate-session";

let cachedWorkspaceRoot: string | null | undefined;

function expandTilde(value: string): string {
  if (value === "~") {
    return os.homedir();
  }
  if (value.startsWith("~/")) {
    return path.join(os.homedir(), value.slice(2));
  }
  return value;
}

function expandEnvVars(value: string): string {
  return value.replace(/\$(\w+)|\$\{([^}]+)\}/g, (_, name, braced) => {
    const key = name || braced;
    return process.env[key] ?? "";
  });
}

function getConfigPath(): string {
  if (process.env.OCTO_CONFIG && process.env.OCTO_CONFIG.length > 0) {
    return process.env.OCTO_CONFIG;
  }

  const xdgConfig = process.env.XDG_CONFIG_HOME;
  const configBase =
    xdgConfig && xdgConfig.length > 0
      ? xdgConfig
      : path.join(os.homedir(), ".config");

  return path.join(configBase, "octo", "config.toml");
}

async function getWorkspaceRoot(): Promise<string | null> {
  if (cachedWorkspaceRoot !== undefined) {
    return cachedWorkspaceRoot;
  }

  try {
    const configPath = getConfigPath();
    const content = await fs.readFile(configPath, "utf-8");
    const match = content.match(/workspace_dir\s*=\s*\"([^\"]+)\"/);
    if (!match) {
      cachedWorkspaceRoot = null;
      return cachedWorkspaceRoot;
    }

    const expanded = expandTilde(expandEnvVars(match[1]));
    cachedWorkspaceRoot = path.resolve(expanded);
    return cachedWorkspaceRoot;
  } catch (error) {
    cachedWorkspaceRoot = null;
    return cachedWorkspaceRoot;
  }
}

export const DelegateSessionsPlugin: Plugin = async ({ client }) => {
  await client.app.log({
    service: SERVICE_NAME,
    level: "info",
    message: "Delegate sessions plugin initialized",
  });

  return {
    tool: {
      delegate_session: tool({
        description:
          "Create a new full OpenCode session and send it a prompt for delegated work.",
        args: {
          directory: tool.schema.string().min(1),
          prompt: tool.schema.string().min(1),
          title: tool.schema.string().optional(),
          model_provider_id: tool.schema.string().optional(),
          model_id: tool.schema.string().optional(),
        },
        async execute(args, ctx) {
          try {
            const workspaceRoot = await getWorkspaceRoot();
            if (!workspaceRoot) {
              return `Unable to resolve workspace_dir from ${getConfigPath()}`;
            }

            const inputDir = expandTilde(args.directory);
            const resolvedDir = path.resolve(
              path.isAbsolute(inputDir)
                ? inputDir
                : path.join(workspaceRoot, inputDir)
            );
            const relativeDir = path.relative(workspaceRoot, resolvedDir);

            if (relativeDir.startsWith("..") || path.isAbsolute(relativeDir)) {
              return `Directory must be within the workspace: ${workspaceRoot}`;
            }

            const headers = { "x-opencode-directory": resolvedDir };

            const session = await (ctx.client.session.create as any)({
              body: {
                title: args.title,
              },
              headers,
            });

            const body: {
              parts: { type: "text"; text: string }[];
              model?: { providerID: string; modelID: string };
            } = {
              parts: [{ type: "text", text: args.prompt }],
            };

            if (args.model_provider_id && args.model_id) {
              body.model = {
                providerID: args.model_provider_id,
                modelID: args.model_id,
              };
            }

            await (ctx.client.session.prompt as any)({
              path: { id: session.id },
              body,
              headers,
            });

            await ctx.client.app.log({
              service: SERVICE_NAME,
              level: "info",
              message: "Delegated prompt to new session",
              extra: {
                session_id: session.id,
                title: args.title ?? null,
                directory: resolvedDir,
              },
            });

            return `Created session ${session.id} and sent the delegated prompt.`;
          } catch (error) {
            await ctx.client.app.log({
              service: SERVICE_NAME,
              level: "error",
              message: "Failed to delegate to new session",
              extra: {
                error: error instanceof Error ? error.message : String(error),
              },
            });

            return "Failed to delegate the prompt to a new session. Check logs for details.";
          }
        },
      }),
    },
  };
};

export default DelegateSessionsPlugin;
