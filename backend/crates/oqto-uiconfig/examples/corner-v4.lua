-- OqtoUI corner-mode v4, expressed only through public customization data.
oqto.setup({
  version = oqto.schema_version,
  preset = "corner-v4",

  appearance = {
    scheme = "oqto-dark",
    radius = "square",
    density = "compact",
  },

  layout = {
    navigator = "left",
    files = "right",
  },

  bindings = {
    { keys = "ctrl+shift+f", action = "view.openFiles" },
    { keys = "ctrl+shift+c", action = "view.openChat" },
  },

  status_line = {
    segments = { "session", "model", "context", "connection", "runnerload" },
  },

  mobile = {
    mode = "corner",
    hold_ms = 320,
    corners = {
      top_left = { tap = "navigator.open", hold = "menu.projects" },
      top_right = { tap = "view.openFiles", hold = "menu.tools" },
      bottom_left = { tap = "session.openPrevious", hold = "menu.sessionMru" },
      bottom_right = { tap = "chat.send", hold = "menu.chatActions" },
    },
  },
})
