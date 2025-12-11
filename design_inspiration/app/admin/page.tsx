"use client"

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Activity, Server, Cpu, HardDrive } from "lucide-react"

export default function AdminPage() {
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
    <div className="p-6 space-y-6 bg-background min-h-screen">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-foreground tracking-wider">ADMIN DASHBOARD</h1>
        <p className="text-sm text-muted-foreground">Platform monitoring and management</p>
      </div>

      {/* System Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <Card className="bg-card border-border">
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-xs text-muted-foreground tracking-wider">CPU USAGE</p>
                <p className="text-2xl font-bold text-foreground font-mono">42%</p>
              </div>
              <Cpu className="w-8 h-8 text-primary" />
            </div>
            <div className="mt-2 h-2 bg-muted rounded-full overflow-hidden">
              <div className="h-full bg-primary" style={{ width: "42%" }}></div>
            </div>
          </CardContent>
        </Card>

        <Card className="bg-card border-border">
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-xs text-muted-foreground tracking-wider">MEMORY</p>
                <p className="text-2xl font-bold text-foreground font-mono">8.4 GB</p>
              </div>
              <HardDrive className="w-8 h-8 text-primary" />
            </div>
            <div className="mt-2 h-2 bg-muted rounded-full overflow-hidden">
              <div className="h-full bg-primary" style={{ width: "53%" }}></div>
            </div>
          </CardContent>
        </Card>

        <Card className="bg-card border-border">
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-xs text-muted-foreground tracking-wider">ACTIVE SESSIONS</p>
                <p className="text-2xl font-bold text-foreground font-mono">{activeSessions.length}</p>
              </div>
              <Activity className="w-8 h-8 text-foreground" />
            </div>
          </CardContent>
        </Card>

        <Card className="bg-card border-border">
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-xs text-muted-foreground tracking-wider">WORKER NODES</p>
                <p className="text-2xl font-bold text-foreground font-mono">1 / 1</p>
              </div>
              <Server className="w-8 h-8 text-foreground" />
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Real-time Metrics Chart */}
      <Card className="bg-card border-border">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-muted-foreground tracking-wider">
            REAL-TIME SYSTEM METRICS
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-48 relative">
            {/* Chart Grid */}
            <div className="absolute inset-0 grid grid-cols-12 grid-rows-6 opacity-20">
              {Array.from({ length: 72 }).map((_, i) => (
                <div key={i} className="border border-border"></div>
              ))}
            </div>

            {/* Chart Lines */}
            <svg className="absolute inset-0 w-full h-full">
              <polyline
                points="0,120 60,110 120,100 180,115 240,105 300,95 360,100 420,90 480,85"
                fill="none"
                stroke="hsl(var(--primary))"
                strokeWidth="2"
              />
              <polyline
                points="0,140 60,135 120,130 180,140 240,135 300,125 360,130 420,120 480,115"
                fill="none"
                stroke="hsl(var(--foreground))"
                strokeWidth="2"
                strokeDasharray="5,5"
              />
            </svg>

            {/* Legend */}
            <div className="absolute top-2 right-2 flex gap-4 text-xs">
              <div className="flex items-center gap-1">
                <div className="w-3 h-0.5 bg-primary"></div>
                <span className="text-muted-foreground">CPU</span>
              </div>
              <div className="flex items-center gap-1">
                <div
                  className="w-3 h-0.5 bg-foreground"
                  style={{
                    backgroundImage:
                      "repeating-linear-gradient(to right, hsl(var(--foreground)) 0, hsl(var(--foreground)) 3px, transparent 3px, transparent 8px)",
                  }}
                ></div>
                <span className="text-muted-foreground">Memory</span>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Active Sessions Table */}
      <Card className="bg-card border-border">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-muted-foreground tracking-wider">ACTIVE SESSIONS</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-border">
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">
                    SESSION ID
                  </th>
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">USER</th>
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">
                    WORKSPACE
                  </th>
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">
                    TEMPLATE
                  </th>
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">
                    DURATION
                  </th>
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">
                    CPU / MEMORY
                  </th>
                  <th className="text-left py-3 px-4 text-xs font-medium text-muted-foreground tracking-wider">
                    ACTIONS
                  </th>
                </tr>
              </thead>
              <tbody>
                {activeSessions.map((session, index) => (
                  <tr
                    key={session.id}
                    className={`border-b border-border hover:bg-accent transition-colors ${
                      index % 2 === 0 ? "bg-card" : "bg-muted"
                    }`}
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
        </CardContent>
      </Card>
    </div>
  )
}
