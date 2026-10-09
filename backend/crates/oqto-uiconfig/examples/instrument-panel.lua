-- Copy to ~/.config/oqto/oqto-ui.lua, then reload OqtoUI.
-- This file can arrange and bind the cockpit; it has no filesystem, network,
-- process, DOM, store, or rendering access.
oqto.setup({
  version = oqto.schema_version,
  preset = "instrument-panel",

  appearance = {
    scheme = "nord-dark",
    radius = "soft",
    density = "compact",
  },

  layout = {
    navigator = "left",
    files = "left",
  },

  bindings = {
    { keys = "ctrl+shift+f", action = "view.openFiles" },
    { keys = "ctrl+shift+c", action = "view.openChat" },
  },

  status_line = {
    segments = { "session", "model", "context", "connection", "runnerload" },
  },
})
