#!/bin/sh
# Prints the CPU model name the host reports, as run-check.pl reads it for the
# time budgets (WHITEFOOT_CPUINFO names another file for tests), or `unknown`.
file=${WHITEFOOT_CPUINFO:-/proc/cpuinfo}
model=$(sed -n '/^model name/{s/^model name[[:space:]]*:[[:space:]]*//;p;q;}' "$file" 2>/dev/null)
printf '%s\n' "${model:-unknown}"
