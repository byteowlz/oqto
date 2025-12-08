export function TerminalView() {
  return (
    <div className="h-full bg-black rounded p-4 font-mono text-sm overflow-y-auto">
      <div className="text-foreground">
        <div className="text-primary">user@agent-workspace:~/project$ npm install</div>
        <div className="text-muted-foreground">added 234 packages in 12s</div>
        <div className="text-primary mt-2">user@agent-workspace:~/project$ npm run dev</div>
        <div className="text-muted-foreground">Server running on http://localhost:3000</div>
        <div className="text-primary mt-2 flex">
          user@agent-workspace:~/project$ <span className="ml-1 animate-pulse">▋</span>
        </div>
      </div>
    </div>
  )
}
