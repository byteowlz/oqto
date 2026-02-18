# Pi Startup Optimization Analysis

## Current Startup Flow

Based on analysis of `packages/coding-agent/src/main.ts`, here's what happens during pi startup:

```
1. Parse CLI args (first pass for extension paths)
2. Create SettingsManager (reads settings.json files)
3. Create AuthStorage
4. Create ModelRegistry (loads models.json, validates schemas)
5. Create ResourceLoader
6. await resourceLoader.reload()  ← TIMED BOTTLENECK
7. Load extensions (may transpile TypeScript, load npm packages)
8. Parse CLI args (second pass with extension flags)
9. Run migrations
10. Create SessionManager
11. Create AgentSession
12. Run mode (RPC/Interactive/Print)
```

## Identified Bottlenecks

### 1. ResourceLoader.reload() (Primary Bottleneck)

**Location:** `src/core/resource-loader.ts`

**What it does:**
- Calls `packageManager.resolve()` (sync + async operations)
- Loads extensions (TypeScript/JavaScript files, npm packages, git repos)
- Loads skills (markdown files)
- Loads prompts (markdown files)
- Loads themes (JSON files)
- Scans project directory tree for AGENTS.md/CLAUDE.md

**Cost:**
- Directory scanning with `readdirSync` and `statSync`
- File reading with `readFileSync`
- Potential npm/git operations if packages need installation
- JSON parsing and schema validation
- Walking up directory tree for context files

### 2. Package Manager.resolve()

**Location:** `src/core/package-manager.ts`

**What it does:**
- Resolves all installed packages (npm, git, local)
- Scans package directories for resources
- Reads package.json files
- Collects resources using ignore patterns (.gitignore, etc.)
- May trigger npm install or git clone if packages missing

**Cost:**
- Synchronous file I/O for package discovery
- Directory traversal with ignore file processing
- External command execution (`npm root -g`)

### 3. Model Registry Initialization

**Location:** `src/core/model-registry.ts`

**What it does:**
- Loads built-in models from pi-ai
- Reads models.json file
- Validates against JSON Schema with Ajv
- Executes shell commands for API key resolution (cached)

**Cost:**
- JSON schema validation (expensive)
- Shell command execution for `!command` API keys
- String manipulation for header resolution

### 4. Settings Manager

**Location:** `src/core/settings-manager.ts`

**What it does:**
- Reads global settings.json from agent directory
- Reads project settings.json from .pi directory
- Deep merges settings

**Cost:**
- Multiple file reads (try-catch with error handling)
- JSON parsing
- Recursive object merging

### 5. Context File Discovery

**Location:** `src/core/resource-loader.ts:loadProjectContextFiles()`

**What it does:**
- Walks up directory tree from cwd to root
- Checks each directory for AGENTS.md or CLAUDE.md
- Stops at root or when files found

**Cost:**
- Multiple directory reads (`readdirSync`)
- File existence checks
- Path resolution

### 6. Migrations

**Location:** `src/migrations.ts` (called from main.ts)

**What it does:**
- Runs migrations on every startup
- May check/modify configuration files

**Cost:**
- File existence checks
- Potential file modifications

## Optimization Opportunities

### Quick Wins (Low Risk, High Impact)

#### 1. Parallelize Independent File Reads

**Current:** Sequential file reads in resource loading

**Proposed:**
```typescript
// Instead of:
const settings = loadSettingsSync();
const models = loadModelsSync();
const extensions = loadExtensionsSync();

// Use:
const [settings, models, extensions] = await Promise.all([
  loadSettings(),
  loadModels(),
  loadExtensions(),
]);
```

**Impact:** 30-50% reduction in I/O-bound startup time

#### 2. Cache Parsed Resources

**Current:** Parse resources (skills, prompts, themes) on every startup

**Proposed:** Add cache file with modification time validation

```typescript
interface ResourceCache {
  version: string;
  skills: Skill[];
  prompts: PromptTemplate[];
  themes: Theme[];
  mtime: number;
}

async function loadCachedResources(): Promise<ResourceCache | null> {
  const cachePath = join(agentDir, '.resource-cache.json');
  if (!existsSync(cachePath)) return null;

  const cache = JSON.parse(readFileSync(cachePath, 'utf-8'));
  if (cache.mtime >= getResourceMaxMtime()) {
    return cache;
  }
  return null;
}
```

**Impact:** 60-80% reduction for repeat startups (no file changes)

#### 3. Lazy Load Extensions

**Current:** Load all extensions during startup

**Proposed:** Load extensions on first use (RPC mode only)

```typescript
class LazyExtensionLoader {
  private loaded = false;

  async getExtension(name: string): Promise<Extension> {
    if (!this.loaded) {
      await this.loadAll();
      this.loaded = true;
    }
    return this.extensions.get(name);
  }
}
```

**Impact:** 40-60% reduction when using only built-in tools

#### 4. Skip Context File Discovery in RPC Mode

**Current:** Always walk directory tree for AGENTS.md/CLAUDE.md

**Proposed:** Skip for RPC mode (Oqto Main Chat uses RPC)

```typescript
// In main.ts
if (parsed.mode === "rpc") {
  resourceLoader.setSkipContextFiles(true);
}
```

**Impact:** 100-500ms (depends on directory depth)

### Medium Effort (Moderate Risk, Moderate Impact)

#### 5. Incremental Package Resolution

**Current:** Scan all packages on every startup

**Proposed:** Track package list and only rescan if changed

```typescript
interface PackageManifest {
  packages: string[];
  lastModified: number;
}

function packagesChanged(manifest: PackageManifest): boolean {
  // Compare manifest with current installed packages
  // Only scan if packages added/removed
}
```

**Impact:** 50-70% reduction in package manager time

#### 6. Pre-validate models.json

**Current:** Validate schema on every startup with Ajv

**Proposed:** Cache validation result, only re-validate if file changes

```typescript
interface ModelsCache {
  valid: boolean;
  models: Model[];
  checksum: string;
  mtime: number;
}
```

**Impact:** 20-30ms reduction

#### 7. Defer Shell Commands for API Keys

**Current:** Execute `!command` during startup (cached)

**Proposed:** Execute on first API call (lazy resolution)

```typescript
// Only execute when actually making API calls
async function getApiKey(provider: string): Promise<string> {
  if (commandCache.has(provider)) {
    return commandCache.get(provider);
  }
  // Execute command now, not at startup
  const result = await execCommand(config.apiKey);
  commandCache.set(provider, result);
  return result;
}
```

**Impact:** 10-100ms (depends on command execution time)

#### 8. Combine Multiple Settings Files

**Current:** Read global settings + project settings separately

**Proposed:** Single read with path resolution

```typescript
// Instead of two separate reads
const settings = await Promise.all([
  loadGlobalSettings(),
  loadProjectSettings(),
]).then(([global, project]) => deepMerge(global, project));
```

**Impact:** 5-10ms reduction

### Advanced Efforts (Higher Risk, Long-term Impact)

#### 9. Worker Thread for Heavy Initialization

**Current:** All startup work on main thread

**Proposed:** Move resource loading to worker thread

```typescript
import { Worker } from 'worker_threads';

async function loadResourcesInBackground(): Promise<LoadResult> {
  return new Promise((resolve, reject) => {
    const worker = new Worker('./resource-loader-worker.js');
    worker.on('message', resolve);
    worker.on('error', reject);
  });
}
```

**Impact:** 200-500ms (unblock main thread, show startup UI faster)

#### 10. Persistent Daemon for Resource Cache

**Current:** Start fresh on every pi invocation

**Proposed:** Background daemon keeps resources loaded

```typescript
// pi-daemon process runs once, resources stay in memory
// pi clients connect to daemon via Unix socket
// Only reload resources when files change (watch mode)
```

**Impact:** 90% reduction (sub-50ms startup for repeat invocations)

#### 11. Binary Snapshot (Bun compile)

**Current:** Node.js runtime + TypeScript interpretation overhead

**Proposed:** Compile to binary with `bun build`

```bash
bun build src/main.ts --compile --outfile pi
```

**Impact:** 50-200ms (faster startup, no module resolution)

## Estimated Impact Summary

| Optimization | Effort | Impact | Risk |
|-------------|--------|--------|------|
| Parallel file reads | Low | 30-50% | Low |
| Cache parsed resources | Low | 60-80%* | Low |
| Lazy load extensions | Low | 40-60% | Medium |
| Skip context files (RPC) | Low | 100-500ms | Low |
| Incremental packages | Medium | 50-70% | Medium |
| Pre-validate models.json | Medium | 20-30ms | Low |
| Defer shell commands | Medium | 10-100ms | Low |
| Worker thread | High | 200-500ms | Medium |
| Persistent daemon | High | 90% | High |
| Binary compilation | Medium | 50-200ms | Low |

*For repeat startups with no file changes

## Recommended Implementation Order

### Phase 1: Quick Wins (Week 1)
1. Skip context file discovery in RPC mode
2. Parallelize SettingsManager and ModelRegistry initialization
3. Defer shell command execution for API keys

### Phase 2: Caching (Week 2)
4. Implement resource cache with mtime validation
5. Cache models.json validation result
6. Add cache invalidation on file change

### Phase 3: Lazy Loading (Week 3)
7. Lazy load extensions
8. Incremental package resolution
9. Benchmark and validate improvements

### Phase 4: Advanced (Future)
10. Evaluate worker thread approach
11. Consider daemon for long-running sessions
12. Benchmark binary compilation

## Testing Strategy

Before/after benchmarking:
```bash
# Measure startup time
time pi --version  # Cold start
time pi --version  # Warm start (cached)
time pi --rpc      # RPC mode startup
```

Add detailed timing instrumentation:
```typescript
import { time } from './timings.js';

time("settings.load");
await loadSettings();
time("settings.load");

// Prints:
// settings.load: 12ms
// resourceLoader.reload: 345ms
// extensions.load: 156ms
```

## Oqto-Specific Recommendations

Since Oqto uses pi in RPC mode for Main Chat:

1. **Skip context file discovery entirely** - Oqto already has project context
2. **Lazy load extensions** - Only load when tools are actually invoked
3. **Disable resource cache** - Oqto manages its own session state
4. **Focus on RPC mode path** - Optimize specifically for `--mode rpc`

Estimated RPC-mode startup time improvement: **300-800ms** → **50-150ms**
