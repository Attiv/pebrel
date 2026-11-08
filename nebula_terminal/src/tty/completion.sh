# Private editor query; existing user bindings retain ownership of F24.
__pebrel_editor_ready_report() {
    if [ "${__pebrel_editor_ready:-0}" = 1 ] && [ -n "${__pebrel_shell_token:-}" ]; then
        printf '\033]1337;SetUserVar=pebrel_editor_ready=%s\007' "$__pebrel_shell_token"
        if typeset -f __pebrel_pointer_ready_report >/dev/null; then __pebrel_pointer_ready_report; fi
    fi
}

__pebrel_editor_report() {
    local encoded owner cursor line prefix suffix unit=utf8
    local argument=${READLINE_ARGUMENT-${NUMERIC-}}
    if [ -n "$argument" ] && typeset -f __pebrel_pointer_action >/dev/null; then
        __pebrel_pointer_action "$argument"
        return
    fi
    owner=$(printf '%s' "$__pebrel_shell_token" | base64 -d)
    if [ -n "${BASH_VERSION-}" ]; then
        line=$READLINE_LINE
        prefix=${READLINE_LINE:0:READLINE_POINT}
    else
        if [[ -n ${widgets[autosuggest-suggest]-} || -n ${POSTDISPLAY-} ]]; then unit=utf8+postdisplay; fi
        suffix=${POSTDISPLAY-}
        line=$BUFFER
        if ((CURSOR == 0)); then prefix=''
        else prefix=${BUFFER[1,CURSOR]}; fi
    fi
    # Native offsets follow the locale's characters; the wire contract is bytes.
    local LC_ALL=C
    cursor=${#prefix}
    if [ "$unit" = utf8+postdisplay ]; then
        # Keep the existing 8 KiB decoded OSC bound and never emit unbounded plugin text.
        (( ${#line} <= 4096 && ${#suffix} <= 4096 && ${#owner}+${#line}+${#suffix} < 8128 )) || return
        encoded=$(printf '%s\n%s\n%s\n%s\n%s' "$owner" "$unit" "$cursor" "$line" "$suffix" | base64 | tr -d '\r\n')
    else
        encoded=$(printf '%s\nutf8\n%s\n%s' "$owner" "$cursor" "$line" | base64 | tr -d '\r\n')
    fi
    printf '\033]1337;SetUserVar=%s=%s\007' "${__pebrel_editor_report_name:-pebrel_editor}" "$encoded"
}

# Bash 3 does not expose the native buffer/caret variables used by bind -x.
if [ -n "${BASH_VERSION-}" ] && (( BASH_VERSINFO[0] >= 4 )) && [[ $- == *i* ]]; then
    __pebrel_editor_bound=0
    while IFS= read -r __pebrel_binding; do
        [[ $__pebrel_binding == '"\e[45~":'* ]] && __pebrel_editor_bound=1
    done < <(bind -p 2>/dev/null; bind -X 2>/dev/null)
    if [[ $__pebrel_editor_bound == 0 ]] && bind -x '"\e[45~":__pebrel_editor_report' 2>/dev/null; then
        __pebrel_editor_ready=1
    fi
    unset __pebrel_editor_bound __pebrel_binding
elif [ -n "${ZSH_VERSION-}" ]; then
    # The private underscore-prefixed widgets are excluded by zsh-autosuggestions.
    __pebrel_editor_hint_changed() {
        if [[ ${__pebrel_editor_last_suffix-} != "${POSTDISPLAY-}" ]]; then
            __pebrel_editor_last_suffix=${POSTDISPLAY-}
            local __pebrel_editor_report_name=pebrel_editor_hint
            __pebrel_editor_report
        fi
    }
    __pebrel_editor_after_autosuggest() {
        zle __pebrel_editor_original_autosuggest -- "$@"
        local result=$?
        __pebrel_editor_hint_changed
        return "$result"
    }
    __pebrel_editor_install_hint_hook() {
        # Async suggestions redraw inside an FD callback, without line-pre-redraw.
        # Preserve the public widget and its arguments; no user key is rebound.
        if [[ ${__pebrel_editor_ready:-0} == 1 && -n ${widgets[autosuggest-suggest]-} &&
              ${widgets[autosuggest-suggest]} != user:__pebrel_editor_after_autosuggest ]]; then
            zle -A autosuggest-suggest __pebrel_editor_original_autosuggest
            zle -N autosuggest-suggest __pebrel_editor_after_autosuggest
        fi
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd __pebrel_editor_install_hint_hook
    autoload -Uz add-zle-hook-widget
    add-zle-hook-widget line-pre-redraw __pebrel_editor_hint_changed

    if [[ $(bindkey '\e[45~') == *undefined-key ]]; then
        zle -N __pebrel_editor_report
        bindkey '\e[45~' __pebrel_editor_report
        __pebrel_editor_ready=1
    fi
fi
