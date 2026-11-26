# The mapping calls of one launch traced by `strace -f -o`, as
# data-placement.sh record compares two launches: each mmap, munmap and mremap
# with its arguments and result, an interrupted call joined to its resumption
# per process, and no process id, so `sort` then `cmp` read two launches as
# mapping alike whatever their process ids and the order their threads ran in.
# brk is left out: the program break follows the image's size, which any change
# of code moves. A trace with no mapping call, or a line that is not the form
# strace writes, is refused (exit 2).
function refuse(message) { print "REFUSED: " message; bad=1; exit 2 }
function unreadable() { refuse("unreadable trace line " FNR ": " $0) }
function keep(call) {
    if (call !~ /^(mmap|munmap|mremap)\(/) return
    # strace aligns results with spaces and may write a space before the
    # closing parenthesis of a resumed call.
    gsub(/[ \t]+/, " ", call)
    gsub(/ \)/, ")", call)
    print call
    kept++
}
{
    if ($1 !~ /^[0-9]+$/) unreadable()
    pid = $1
    rest = $0
    sub(/^[ \t]*[0-9]+[ \t]+/, "", rest)
    if (rest ~ /^\+\+\+ .* \+\+\+$/ || rest ~ /^--- .* ---$/) next
    if (rest ~ /^<\.\.\. [a-z_0-9]+ resumed>/) {
        name = rest
        sub(/^<\.\.\. /, "", name)
        sub(/ resumed>.*/, "", name)
        if (!(pid in pending) || pending_name[pid] != name) unreadable()
        sub(/^<\.\.\. [a-z_0-9]+ resumed>/, "", rest)
        call = pending[pid] rest
        delete pending[pid]
        delete pending_name[pid]
        if (call !~ /\) += /) unreadable()
        keep(call)
        next
    }
    if (rest !~ /^[a-z_0-9]+\(/) unreadable()
    if (rest ~ /<unfinished \.\.\.>$/) {
        if (pid in pending) unreadable()
        sub(/[ \t]*<unfinished \.\.\.>$/, "", rest)
        pending[pid] = rest
        name = rest
        sub(/\(.*/, "", name)
        pending_name[pid] = name
        next
    }
    if (rest !~ /\) += /) unreadable()
    keep(rest)
}
END {
    if (bad) exit 2
    for (pid in pending) refuse("process " pid " left " pending_name[pid] " unfinished")
    if (!kept) refuse("no mapping call")
}
