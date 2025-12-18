# Path setup
export GOPATH=$HOME/go
export PATH=$HOME/.opencode/bin:$HOME/.cargo/bin:$GOPATH/bin:$HOME/.local/bin:$PATH

# Shell integrations
eval "$(starship init zsh)"
eval "$(zoxide init --cmd cd zsh)"
source <(fzf --zsh)

# Completions
eval "$(uv generate-shell-completion zsh)"
eval "$(uvx --generate-shell-completion zsh)"

# Environment
export EDITOR='nvim'
export VISUAL='nvim'
export FZF_DEFAULT_COMMAND='rg --files --hidden --glob "!.git/" --glob "!node_modules/"'

# Aliases
alias vim='nvim'
alias vi='nvim'
