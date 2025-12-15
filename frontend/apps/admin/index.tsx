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
    <div className="flex flex-col gap-4 h-full min-h-0 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold text-foreground tracking-wider">ADMIN DASHBOARD</h1>
        <p className="text-sm text-muted-foreground">Platform monitoring and management</p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {[{ label: "CPU USAGE", value: "42%", Icon: Cpu }, { label: "MEMORY", value: "8.4 GB", Icon: HardDrive }, { label: "ACTIVE SESSIONS", value: activeSessions.length, Icon: Activity }, { label: "WORKER NODES", value: "1 / 1", Icon: Server }].map(
          ({ label, value, Icon }, idx) => (
            <div key={label} className="bg-[#161c1a] border border-[#1f2a27] rounded-xl p-4 flex flex-col gap-3 hover:border-[#3ba77c] hover:bg-[#131a17] transition">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs text-muted-foreground tracking-wider">{label}</p>
                  <p className="text-2xl font-bold text-foreground font-mono">{value}</p>
                </div>
                <Icon className={`w-8 h-8 ${idx === 2 || idx === 3 ? "text-foreground" : "text-[#3ba77c]"}`} />
              </div>
              <div className="mt-1 h-2 bg-[#0f1412] rounded-full overflow-hidden">
                <div className="h-full bg-[#3ba77c]/70" style={{ width: idx === 0 ? "42%" : idx === 1 ? "53%" : "100%" }}></div>
              </div>
            </div>
          ),
        )}
      </div>

      <div className="bg-[#161c1a] border border-[#1f2a27] rounded-xl">
        <div className="border-b border-[#1f2a27] px-4 py-3">
          <h2 className="text-sm font-semibold text-muted-foreground tracking-wider">REAL-TIME SYSTEM METRICS</h2>
        </div>
        <div className="p-4">
          <div className="h-48 relative">
            <div className="absolute inset-0 grid grid-cols-12 grid-rows-6 opacity-20">
              {Array.from({ length: 72 }).map((_, i) => (
                <div key={i} className="border border-[#1f2a27]"></div>
              ))}
            </div>
            <svg className="absolute inset-0 w-full h-full">
              <polyline
                points="0,120 60,110 120,100 180,115 240,105 300,95 360,100 420,90 480,85"
                fill="none"
                stroke="var(--primary)"
                strokeWidth="2"
              />
              <polyline
                points="0,140 60,135 120,130 180,140 240,135 300,125 360,130 420,120 480,115"
                fill="none"
                stroke="var(--foreground)"
                strokeWidth="2"
                strokeDasharray="5,5"
              />
            </svg>
            <div className="absolute top-2 right-2 flex gap-4 text-xs">
              <div className="flex items-center gap-1">
                <div className="w-3 h-0.5 bg-[#3ba77c]"></div>
                <span className="text-muted-foreground">CPU</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="w-3 h-0.5 bg-foreground" style={{ backgroundImage: "repeating-linear-gradient(to right, currentColor 0, currentColor 3px, transparent 3px, transparent 8px)" }}></div>
                <span className="text-muted-foreground">Memory</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="bg-[#161c1a] border border-[#1f2a27] rounded-xl">
        <div className="border-b border-[#1f2a27] px-4 py-3">
          <h2 className="text-sm font-semibold text-muted-foreground tracking-wider">ACTIVE SESSIONS</h2>
        </div>
        <div className="p-4 overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="border-b border-[#1f2a27]">
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
                  className={`border-b border-[#1f2a27] transition ${
                    index % 2 === 0 ? "bg-[#0f1412]" : "bg-[#131a17]"
                  } hover:border-[#3ba77c] hover:bg-[#131a17]`}
                >
                  <td className="py-3 px-4 text-sm text-foreground font-mono">{session.id}</td>
                  <td className="py-3 px-4 text-sm text-muted-foreground">{session.user}</td>
                  <td className="py-3 px-4 text-sm text-muted-foreground">{session.workspace}</td>
                  <td className="py-3 px-4">
                    <span className="text-xs px-2 py-1 bg-primary/20 text-primary rounded">{session.template}</span>
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
