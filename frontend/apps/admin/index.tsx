"use client"

import { Button } from "@/components/ui/button"
import { Activity, Server, Cpu, HardDrive } from "lucide-react"

export function AdminApp() {
  const activeSessions = [
    {
      id: "sess-001",
      user: "john.doe@company.com",
      workspace: "Customer Analytics",
      template: "Coding Copilot",
      duration: "42m 15s",
      cpu: "45%",
      memory: "1.2 GB",
    },
    {
      id: "sess-002",
      user: "jane.smith@company.com",
      workspace: "Q4 Reports",
      template: "Research Assistant",
      duration: "1h 23m",
      cpu: "12%",
      memory: "456 MB",
    },
    {
      id: "sess-003",
      user: "bob.wilson@company.com",
      workspace: "API Integration",
      template: "Coding Copilot",
      duration: "15m 08s",
      cpu: "67%",
      memory: "2.1 GB",
    },
  ]

  return (
    <div className="flex flex-col gap-4 h-full min-h-0 p-4 md:p-6 overflow-y-auto w-full">
      <div>
        <h1 className="text-xl md:text-2xl font-bold text-foreground tracking-wider">ADMIN DASHBOARD</h1>
        <p className="text-sm text-muted-foreground">Platform monitoring and management</p>
      </div>

      {/* Stats Grid - 2 columns on mobile, 4 on desktop */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 md:gap-4 w-full">
        {[
          { label: "CPU USAGE", value: "42%", Icon: Cpu, percent: 42 },
          { label: "MEMORY", value: "8.4 GB", Icon: HardDrive, percent: 53 },
          { label: "ACTIVE SESSIONS", value: activeSessions.length, Icon: Activity, percent: 100 },
          { label: "WORKER NODES", value: "1 / 1", Icon: Server, percent: 100 },
        ].map(({ label, value, Icon, percent }, idx) => (
          <div key={label} className="bg-card border border-border p-3 md:p-4 flex flex-col gap-2 md:gap-3 hover:border-primary transition">
            <div className="flex items-center justify-between">
              <div className="min-w-0">
                <p className="text-[10px] md:text-xs text-muted-foreground tracking-wider truncate">{label}</p>
                <p className="text-lg md:text-2xl font-bold text-foreground font-mono">{value}</p>
              </div>
              <Icon className={`w-6 h-6 md:w-8 md:h-8 shrink-0 ${idx === 2 || idx === 3 ? "text-foreground" : "text-primary"}`} />
            </div>
            <div className="h-1.5 md:h-2 bg-muted overflow-hidden">
              <div className="h-full bg-primary/70" style={{ width: `${percent}%` }}></div>
            </div>
          </div>
        ))}
      </div>

      {/* Chart Section */}
      <div className="bg-card border border-border">
        <div className="border-b border-border px-3 md:px-4 py-2 md:py-3">
          <h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">REAL-TIME SYSTEM METRICS</h2>
        </div>
        <div className="p-3 md:p-4">
          <div className="h-32 md:h-48 relative">
            <div className="absolute inset-0 grid grid-cols-6 md:grid-cols-12 grid-rows-4 md:grid-rows-6 opacity-20">
              {Array.from({ length: 24 }).map((_, i) => (
                <div key={i} className="border border-border"></div>
              ))}
            </div>
            <svg className="absolute inset-0 w-full h-full" viewBox="0 0 480 150" preserveAspectRatio="none">
              <polyline
                points="0,100 60,90 120,80 180,95 240,85 300,75 360,80 420,70 480,65"
                fill="none"
                stroke="var(--primary)"
                strokeWidth="2"
                vectorEffect="non-scaling-stroke"
              />
              <polyline
                points="0,120 60,115 120,110 180,120 240,115 300,105 360,110 420,100 480,95"
                fill="none"
                stroke="var(--foreground)"
                strokeWidth="2"
                strokeDasharray="5,5"
                vectorEffect="non-scaling-stroke"
              />
            </svg>
            <div className="absolute top-2 right-2 flex gap-3 md:gap-4 text-[10px] md:text-xs">
              <div className="flex items-center gap-1">
                <div className="w-2 md:w-3 h-0.5 bg-primary"></div>
                <span className="text-muted-foreground">CPU</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="w-2 md:w-3 h-0.5 bg-foreground opacity-50"></div>
                <span className="text-muted-foreground">Memory</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Sessions Section */}
      <div className="bg-card border border-border">
        <div className="border-b border-border px-3 md:px-4 py-2 md:py-3">
          <h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">ACTIVE SESSIONS</h2>
        </div>
        
        {/* Mobile: Card Layout */}
        <div className="md:hidden p-3 space-y-3">
          {activeSessions.map((session) => (
            <div key={session.id} className="border border-border p-3 space-y-2">
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="text-xs font-mono text-foreground">{session.id}</p>
                  <p className="text-xs text-muted-foreground truncate">{session.user}</p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="border-destructive text-destructive hover:bg-destructive/10 bg-transparent h-7 text-xs shrink-0"
                >
                  Kill
                </Button>
              </div>
              <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
                <span className="text-muted-foreground">
                  <span className="text-foreground">{session.workspace}</span>
                </span>
                <span className="px-1.5 py-0.5 bg-primary/20 text-primary text-[10px]">{session.template}</span>
              </div>
              <div className="flex gap-4 text-xs text-muted-foreground">
                <span>{session.duration}</span>
                <span>{session.cpu} / {session.memory}</span>
              </div>
            </div>
          ))}
        </div>

        {/* Desktop: Table Layout */}
        <div className="hidden md:block p-4 overflow-x-auto">
          <table className="w-full min-w-[700px]">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">SESSION ID</th>
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">USER</th>
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">WORKSPACE</th>
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">TEMPLATE</th>
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">DURATION</th>
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">CPU / MEMORY</th>
                <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">ACTIONS</th>
              </tr>
            </thead>
            <tbody>
              {activeSessions.map((session, index) => (
                <tr
                  key={session.id}
                  className={`border-b border-border transition ${
                    index % 2 === 0 ? "bg-muted/30" : "bg-muted/10"
                  } hover:border-primary hover:bg-muted/50`}
                >
                  <td className="py-3 px-4 text-sm text-foreground font-mono">{session.id}</td>
                  <td className="py-3 px-4 text-sm text-muted-foreground">{session.user}</td>
                  <td className="py-3 px-4 text-sm text-muted-foreground">{session.workspace}</td>
                  <td className="py-3 px-4">
                    <span className="text-xs px-2 py-1 bg-primary/20 text-primary">{session.template}</span>
                  </td>
                  <td className="py-3 px-4 text-sm text-muted-foreground font-mono">{session.duration}</td>
                  <td className="py-3 px-4 text-sm text-muted-foreground font-mono">
                    {session.cpu} / {session.memory}
                  </td>
                  <td className="py-3 px-4">
                    <Button
                      variant="outline"
                      size="sm"
                      className="border-destructive text-destructive hover:bg-destructive/10 bg-transparent"
                    >
                      Kill
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  )
}

export default AdminApp
