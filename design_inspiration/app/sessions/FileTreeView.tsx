export function FileTreeView() {
  const files = [
    { name: "src", type: "folder", children: ["index.ts", "auth.ts", "database.ts"] },
    { name: "package.json", type: "file" },
    { name: "README.md", type: "file" },
  ]

  return (
    <div className="h-full overflow-y-auto bg-background/50 border border-border rounded p-4">
      <div className="space-y-1 text-sm">
        <div className="text-muted-foreground font-mono">📁 project-root/</div>
        <div className="ml-4 space-y-1">
          <div className="text-muted-foreground font-mono cursor-pointer hover:text-primary">📁 src/</div>
          <div className="ml-4 space-y-1">
            <div className="text-foreground font-mono cursor-pointer hover:text-primary">📄 index.ts</div>
            <div className="text-foreground font-mono cursor-pointer hover:text-primary">📄 auth.ts</div>
            <div className="text-foreground font-mono cursor-pointer hover:text-primary">📄 database.ts</div>
          </div>
          <div className="text-foreground font-mono cursor-pointer hover:text-primary">📄 package.json</div>
          <div className="text-foreground font-mono cursor-pointer hover:text-primary">📄 README.md</div>
        </div>
      </div>
    </div>
  )
}
