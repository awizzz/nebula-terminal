# Nebula Terminal shell integration for zsh.
#
# Zsh reads this file as its .zshenv, through ZDOTDIR. It puts your ZDOTDIR back and
# reads your own .zshenv, so .zprofile, .zshrc and the rest come from your folder as
# always. Then it marks each prompt and command (OSC 133), which command jumps and
# notifications use, and reports the current folder (OSC 7) for new tabs and splits.

if [[ -n ${NEBULA_ZDOTDIR-} ]]; then
  ZDOTDIR=$NEBULA_ZDOTDIR
else
  unset ZDOTDIR
fi
unset NEBULA_ZDOTDIR
if [[ -r ${ZDOTDIR:-$HOME}/.zshenv ]]; then
  builtin source "${ZDOTDIR:-$HOME}/.zshenv"
fi

if [[ -o interactive ]]; then
  __nebula_precmd() {
    local exit_status=$?
    builtin printf '\e]133;D;%s\a' "$exit_status"
    builtin printf '\e]7;file://%s%s\a' "${HOST:-localhost}" "${PWD//\%/%25}"
    builtin printf '\e]133;A\a'
  }
  __nebula_preexec() {
    builtin printf '\e]133;C\a'
  }
  autoload -Uz add-zsh-hook
  add-zsh-hook precmd __nebula_precmd
  add-zsh-hook preexec __nebula_preexec
fi
