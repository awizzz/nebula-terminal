# Nebula Terminal shell integration for bash (Git Bash and WSL).
#
# Bash runs this file instead of its usual startup files, so it reads them first,
# as a login shell would. Then it marks each prompt and command (OSC 133), which
# command jumps and notifications use, and reports the current folder (OSC 7) for
# new tabs and splits.

if [ -r /etc/profile ]; then . /etc/profile; fi
if [ -r ~/.bash_profile ]; then . ~/.bash_profile
elif [ -r ~/.bash_login ]; then . ~/.bash_login
elif [ -r ~/.profile ]; then . ~/.profile
fi

__nebula_folder_pwd=
__nebula_folder_url=

# Turns $PWD into a file URL Windows understands: C:/Users/me for drives, and the
# distribution's share (\\wsl$\Ubuntu\home\me) for the rest of a WSL file system.
__nebula_update_folder() {
  __nebula_folder_pwd=$PWD
  __nebula_folder_url=
  local folder=
  case $PWD in
    *[[:cntrl:]]*) return ;;
  esac
  if [ -n "${WSL_DISTRO_NAME-}" ]; then
    case $PWD in
      /mnt/[a-zA-Z] | /mnt/[a-zA-Z]/*) folder="${PWD:5:1}:${PWD:6}" ;;
      *) __nebula_folder_url="file://wsl\$/$WSL_DISTRO_NAME$PWD" ;;
    esac
  elif [ -n "${MSYSTEM-}" ]; then
    case $PWD in
      /[a-zA-Z] | /[a-zA-Z]/*) folder="${PWD:1:1}:${PWD:2}" ;;
      *) folder=$(builtin pwd -W 2>/dev/null) ;;
    esac
  fi
  if [ -n "$folder" ]; then
    case $folder in
      ?:) folder="$folder/" ;;
    esac
    __nebula_folder_url="file://${HOSTNAME:-localhost}/${folder^}"
  fi
  __nebula_folder_url=${__nebula_folder_url//[%]/%25}
}

__nebula_precmd() {
  local status=$?
  builtin printf '\e]133;D;%s\a' "$status"
  if [ "$PWD" != "$__nebula_folder_pwd" ]; then __nebula_update_folder; fi
  if [ -n "$__nebula_folder_url" ]; then builtin printf '\e]7;%s\a' "$__nebula_folder_url"; fi
  return "$status"
}

# Runs after your own PROMPT_COMMAND, so a prompt it rebuilds still gets its mark.
__nebula_mark_prompt() {
  case $PS1 in
    *'133;A'*) ;;
    *) PS1='\[\e]133;A\a\]'$PS1 ;;
  esac
}

PROMPT_COMMAND=$'__nebula_precmd\n'"${PROMPT_COMMAND-}"$'\n__nebula_mark_prompt'
PS0="${PS0-}"'\e]133;C\a'
