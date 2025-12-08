# Agent Configuration

## About <name>

- Name: <name>
- Role: <role> at Fraunhofer IEM
- Studied <studies>
- Has ...

## Directory Structure

- **Desktop**: Screenshots and videos for reference
- **~/Code**: Main coding projects
- **~/Documents**: Personal videos + Documents
- **~/Downloads**: Recent downloads

## Working Directories

- **Scripts**: '~/scripts/' - Custom scripts and automation (currently subfolders for bash, python and applescript but can be extended to other languages if necessary)
- **To dos**: '~/lists' - Things to do, reminders, etc (markdown with yaml frontmatter, use the lst cli to manage)
- **Memory**: '~/notes/memories' - Important information to remember (markdown with yaml frontmatter)
- **Journal**: '~/notes/journal' - Personal journal entries (markdown with yaml frontmatter)
- **Ideas**: '~/ideas' - Creative ideas and thoughts (markdown with yaml frontmatter)
- **Projects**: '~/notes/projects' - Active projects I'm working on (markdown with yaml frontmatter)

## Instructions & Projects

You should search memories to see if there's relevant information for my query (mmry).
As I do work, use mmry cli to store information that you need to remember for the project.

## Document Layout

lists:

```markdown
---
id: da9295f6-b05d-4f5c-b2b6-85d6cb405622
title: cli
sharing: []
updated: 2025-06-26T20:17:49.781001Z
---

- [ ] ncdu: NCurses Disk Usage. ^YUqF3
- [x] duf: Disk Usage/Free (prettier version of df). ^zmuyY
- [x] ripgrep (or rg): A very fast grep alternative. ^MGMeK
- [x] mosh: Mobile Shell (like SSH but supports roaming and keeps session open on network changes). ^fDMT3
- [ ] lshw: List Hardware (shows detailed hardware information). ^9oIcS
- [ ] mtr: My Traceroute (combines ping and traceroute into a live diagnostic tool). ^0ruKt
- [ ] fd: A simple, fast, and user-friendly alternative to find. ^axxAz
```

notes:

## Tools

<name> created his own todo list and notes app called lst. Here is how you can use it:

```bash
lst add <list_name> <item_name(s)> #you can add multiple items by separating them with a comma but make sure everything is enclose in quotation marks. List names are fuzzy found, so even incomplete names can result in a match
```

Examples:

```terminal
❯ lst add opensh "opencode"
Added to openshovelshack: opencode

```

Here are all currently available commands:

```bash
Personal lists & notes app

Usage: lst [OPTIONS] <COMMAND>

Commands:
  ls       List all lists or show contents of a specific list
  new      Create and open a new list
  add      Add an item to a list
  open     Open a list in the editor
  done     Mark an item as done
  undone   Mark a completed item as not done
  rm       Delete item from a list
  wipe     Delete all entries from a list
  pipe     Read items from stdin and add them to a list
  note     Commands for managing notes
  img      Commands for managing images
  dl       Daily list commands (add, done, or display)
  dn       Daily note: create or open today's note
  sync     Sync daemon commands
  share    Share a document with other devices
  unshare  Remove sharing information from a document
  gui      Send commands to a running lst-desktop instance
  tidy     Tidy all lists: ensure proper YAML frontmatter and formatting
  auth     Authentication commands for server access
  server   Server content management commands
  help     Print this message or the help of the given subcommand(s)

Options:
      --json     Output in JSON format
  -h, --help     Print help
  -V, --version  Print version

```

For general memories you can use the mmry tool, here is how to use it:

```bash

❯ mmry --help
A lean, local-first memory management system

Usage: mmry [OPTIONS] <COMMAND>

Commands:
  add        Add a new memory
  search     Search memories
  ls         List memories
  stats      Show statistics
  reembed    Regenerate embeddings for existing memories

Options:
      --debug    Enable debug logging
  -h, --help     Print help
  -V, --version  Print version
```
