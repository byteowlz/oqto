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

if command -v zoxide >/dev/null 2>&1; then
  eval "$(zoxide init bash)"
fi

if command -v starship >/dev/null 2>&1; then
  eval "$(starship init bash)"
fi
