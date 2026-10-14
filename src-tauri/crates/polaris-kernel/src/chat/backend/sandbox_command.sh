# Container supervisor. Only BusyBox/POSIX utilities; no host shell execution.
# Arguments: shell, command, timeout seconds, unique response marker.
exec 2>&1
shell=$1
command=$2
duration=$3
marker=$4
state=$(mktemp -d) || exit 125
trap 'rm -rf "$state"' EXIT
# EXIT runs for normal exits (including explicit exit 137), but not SIGKILL.
# A separate session allows the watchdog to terminate descendants too.
setsid "$shell" -c '
  trap '\''code=$?; printf "%s" "$code" > "$2"'\'' EXIT
  eval "$1"
' polaris-command "$command" "$state/exited" &
child=$!
setsid sh -c '
  sleep "$1"
  : > "$2/timeout"
  kill -KILL "-$3" 2>/dev/null
' polaris-watchdog "$duration" "$state" "$child" </dev/null >/dev/null 2>&1 &
watchdog=$!
wait "$child" 2>"$state/wait"
code=$?
kill -KILL "-$watchdog" 2>/dev/null
kill -KILL "$watchdog" 2>/dev/null
wait "$watchdog" 2>/dev/null
signal=0
timed_out=false
if [ -f "$state/timeout" ]; then
  timed_out=true
  signal=9
# wait reports signal death separately from a normal high exit code. The
# EXIT marker also covers shells that run EXIT while handling a signal.
elif [ "$(cat "$state/exited" 2>/dev/null)" != "$code" ] && [ "$code" -gt 128 ] && [ -s "$state/wait" ]; then
  signal=$((code - 128))
fi
printf '\n%s:%s:%s:%s\n' "$marker" "$code" "$signal" "$timed_out"
