# Intentional obsolete Todo fixture — never install

Exact `pi-oqto-todos/index.ts` from byteowlz/pi-agent-extensions commit
`c19bc0d804976bd7add1d185e34ade85f9842030` (SHA256
`30dc9d04507ad31b72938770cf6ffbf955a92adf502fff67a620975d80f1c3cb`).
This is vendor extension code, not a user's Todo data or configuration.

`test-pi-extension-supersession.py` copies it only into a disposable profile to
reproduce the actual Pi autoload collision with current `pi-todolist`. Preserve
its exact bytes: changing the fixture invalidates the observed-asset regression.
It is deliberately outside every production autoload/install manifest.
