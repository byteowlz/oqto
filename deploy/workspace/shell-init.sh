# Oqto workspace interactive-shell defaults.
# System-wide because the durable /home/oqto mount must preserve user-owned
# dotfiles instead of replacing them with image defaults.
case $- in
  *i*) ;;
  *) return ;;
esac

# Keep familiar commands available while using the modern implementations.
if command -v eza >/dev/null 2>&1; then
  alias ls='eza --group-directories-first'
  alias ll='eza --group-directories-first --long --all --git'
fi

if [ -n "${ZSH_VERSION:-}" ]; then
  _oqto_shell=zsh
else
  _oqto_shell=bash
fi

if command -v zoxide >/dev/null 2>&1; then
  eval "$(zoxide init "$_oqto_shell")"
fi

if command -v starship >/dev/null 2>&1; then
  eval "$(starship init "$_oqto_shell")"
fi

unset _oqto_shell
