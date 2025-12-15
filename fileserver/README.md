# Fileserver

Lightweight Rust file server for container workspace access. Designed to run alongside opencode in containers, providing file browsing, upload, download, and deletion capabilities.

## Features

- Directory tree listing with configurable depth
- Two view modes:
  - **Full**: Complete directory tree with all files
  - **Simple**: Flat list of office/document files only (for non-technical users)
- File upload with multipart support
- File download with proper MIME types
- File and directory deletion
- Directory creation
- Path traversal protection
- Configurable hidden files/directories
- CORS enabled for frontend access

## Installation

```bash
cd fileserver
cargo build --release
```

The binary will be at `target/release/fileserver`.

## Usage

```bash
# Basic usage - serve current directory on port 41821
fileserver

# Custom port and directory
fileserver --port 8080 --root /path/to/serve

# With config file
fileserver --config /path/to/config.toml

# Verbose logging
fileserver --verbose
```

### CLI Options

| Option | Env Var | Default | Description |
|--------|---------|---------|-------------|
| `-p, --port` | `FILESERVER_PORT` | 41821 | Port to listen on |
| `-b, --bind` | `FILESERVER_BIND` | 0.0.0.0 | Address to bind to |
| `-r, --root` | `FILESERVER_ROOT` | . | Root directory to serve |
| `-c, --config` | `FILESERVER_CONFIG` | - | Config file path |
| `-v, --verbose` | `FILESERVER_VERBOSE` | false | Enable verbose logging |

## API Endpoints

### Health Check

```
GET /health
```

Response:
```json
{
  "status": "ok",
  "root": "/path/to/workspace"
}
```

### Directory Tree

```
GET /tree?path=<path>&mode=<mode>&depth=<depth>&show_hidden=<bool>
```

Parameters:
- `path` - Relative path (default: ".")
- `mode` - `full` (default) or `simple`
- `depth` - Maximum depth (default: from config)
- `show_hidden` - Include hidden files (default: false)

Response:
```json
[
  {
    "name": "src",
    "path": "src",
    "type": "directory",
    "modified": 1702656000,
    "children": [...]
  },
  {
    "name": "README.md",
    "path": "README.md",
    "type": "file",
    "size": 1234,
    "modified": 1702656000
  }
]
```

### Get File Content

```
GET /file?path=<path>
```

Returns the file content with appropriate MIME type.

### Upload File

```
POST /file?path=<destination>&mkdir=<bool>
Content-Type: multipart/form-data
```

Parameters:
- `path` - Destination path (file path or directory)
- `mkdir` - Create parent directories if needed (default: false)

Body: multipart form with file field

Response:
```json
{
  "success": true,
  "message": "File uploaded: filename.txt",
  "path": "uploads/filename.txt"
}
```

### Delete File/Directory

```
DELETE /file?path=<path>
```

Response:
```json
{
  "success": true,
  "message": "Deleted: path/to/file",
  "path": "path/to/file"
}
```

### Create Directory

```
PUT /mkdir?path=<path>
```

Response:
```json
{
  "success": true,
  "message": "Created directory: new/dir",
  "path": "new/dir"
}
```

## Configuration

See `examples/config.toml` for all options:

```toml
# Maximum upload size (100 MB)
max_upload_size = 104857600

# Maximum tree depth
max_depth = 20

# Hidden directories (excluded from listings)
hidden_dirs = [".git", "node_modules", "__pycache__", "target"]

# Hidden file extensions
hidden_extensions = [".pyc", ".o", ".so"]

# Office file extensions (for simple mode)
office_extensions = [".pdf", ".docx", ".xlsx", ".csv", ".txt", ".md"]
```

## Running with Opencode

Use the wrapper script to start both services together:

```bash
./scripts/start-with-opencode.sh --root /workspace
```

Or set environment variables:

```bash
export OPENCODE_PORT=4096
export FILESERVER_PORT=4097  # or leave unset for auto (opencode + 1)
export FILESERVER_ROOT=/workspace
./scripts/start-with-opencode.sh
```

## Container Integration

In the container's entrypoint:

```bash
# Start fileserver on port 41821, serving /home/dev/workspace
fileserver --port 41821 --root /home/dev/workspace &

# Start opencode on port 41820
opencode serve --port 41820
```

## Security

- Path traversal attacks are blocked (cannot escape root directory)
- Configurable file size limits
- No authentication (handled by orchestrator/proxy layer)
