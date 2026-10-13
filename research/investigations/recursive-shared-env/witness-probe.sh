#!/bin/sh
# Temporary helper of .github/workflows/tmp-witness-probe.yml; removed with it
# before the branch is ready.
#
#   witness-probe.sh wrapper REAL_CLANG OUT
#       writes the clang wrapper to OUT. It passes every call through to
#       REAL_CLANG, except that with WF_CC set to x86_regcallcc or
#       preserve_nonecc it first rewrites each `-x ir` input module: the
#       `define` and `declare` lines of wf_{step,compound,test}_with[out] get
#       that calling convention and so do the `call` lines to them. It logs
#       the rewritten signatures to $WF_CC_LOG and keeps the rewritten module
#       in $WF_CC_KEEP.
#   witness-probe.sh measure SNOWGHOST COMPILER NAME RESULTS
#       runs Snowghost's `scan-param.sh witness` with COMPILER for the three
#       variants (plain, regcall, preserve_none) and files each one's outputs
#       under RESULTS/NAME-VARIANT with a summary.tsv.
#   witness-probe.sh report RESULTS NAME...
#       prints, per variant, the table of function, calls, instructions per
#       call for the copies with and without the table, one column group per
#       NAME.
set -u

here=$(cd "$(dirname "$0")" && pwd)

wrapper() {
	real=$1 out=$2
	cat >"$out" <<WRAP
#!/bin/sh
real=$real
case \${WF_CC:-} in
x86_regcallcc | preserve_nonecc) ;;
*) exec "\$real" "\$@" ;;
esac
lang=
expect=
for argument in "\$@"; do
	if [ -n "\$expect" ]; then
		lang=\$argument
		expect=
		continue
	fi
	case \$argument in
	-x) expect=1 ;;
	*.ll)
		if [ "\$lang" = ir ]; then
			rewritten=\$argument.rewritten
			awk -v cc="\$WF_CC" -v logfile="\$WF_CC_LOG" -v file="\$argument" '
			BEGIN {
				linkage = "^(private|internal|available_externally|linkonce_odr|linkonce|weak_odr|weak|common|appending|extern_weak|external|dso_local|dso_preemptable|hidden|protected|default) "
				existing = "^(ccc|fastcc|coldcc|x86_regcallcc|preserve_nonecc|preserve_mostcc|preserve_allcc|cc [0-9]+) "
				name = "@\"?wf_(step|compound|test)_with(out)?([.][A-Za-z0-9_.]*)?\"?[(]"
			}
			function strip(s, prefix) {
				while (match(s, existing)) s = substr(s, RLENGTH + 1)
				return s
			}
			/^(define|declare) / && \$0 ~ name {
				kind = (\$1 == "define") ? "define" : "declare"
				rest = substr(\$0, length(kind) + 2)
				prefix = ""
				while (match(rest, linkage)) { prefix = prefix substr(rest, 1, RLENGTH); rest = substr(rest, RLENGTH + 1) }
				rest = strip(rest)
				\$0 = kind " " prefix cc " " rest
				if (kind == "define") defines++; else declares++
				print file ": " \$0 >> logfile
				print
				next
			}
			/ call / && \$0 ~ name {
				i = index(\$0, " call ")
				head = substr(\$0, 1, i + 5)
				rest = strip(substr(\$0, i + 6))
				\$0 = head cc " " rest
				calls++
			}
			{ print }
			END { print file ": rewritten " (defines + 0) " defines, " (declares + 0) " declares, " (calls + 0) " calls to " cc >> logfile }
			' "\$argument" >"\$rewritten" || exit 1
			cp "\$rewritten" "\$WF_CC_KEEP/\$(date +%s%N)-\$(basename "\$argument")"
			mv "\$rewritten" "\$argument"
		fi
		;;
	esac
done
exec "\$real" "\$@"
WRAP
	chmod +x "$out"
}

# One variant's raw outputs to a summary: variant, function, copy, calls,
# exclusive instructions per call, inclusive instructions per call.
summarize() {
	dir=$1 variant=$2
	awk -v variant="$variant" '
	function num(s) { gsub(",", "", s); return s + 0 }
	NR > 1 && NF >= 5 {
		name = $5
		sub(/[.].*/, "", name)
		if (name !~ /^wf_(step|compound|test)_with(out)?$/) next
		calls[name] += num($1)
		excl[name] += num($2)
		incl[name] += num($3)
	}
	END {
		for (name in calls) {
			function_name = name; sub(/^wf_/, "", function_name)
			copy = (function_name ~ /_without$/) ? "without" : "with"
			sub(/_with(out)?$/, "", function_name)
			if (calls[name] > 0) printf "%s\t%s\t%s\t%d\t%.2f\t%.2f\n", variant, function_name, copy, calls[name], excl[name] / calls[name], incl[name] / calls[name]
			else printf "%s\t%s\t%s\t0\t-\t-\n", variant, function_name, copy
		}
	}' "$dir/calls.txt"
	# main: each instruction line run once per call of step (1,048,576
	# times), attributed to the copy whose call of step is nearest in
	# listing order.
	awk -v variant="$variant" '
	/^== / { inmain = ($2 == "wf_main:"); next }
	inmain && $1 == "1,048,576" {
		n++
		copy[n] = ""
		if ($0 ~ /call.*<wf_step_with(\.[^>]*)?>/) copy[n] = "with"
		if ($0 ~ /call.*<wf_step_without(\.[^>]*)?>/) copy[n] = "without"
	}
	END {
		for (i = 1; i <= n; i++) {
			best = ""; distance = 1e9
			for (j = 1; j <= n; j++) if (copy[j] != "" && (i > j ? i - j : j - i) < distance) { distance = (i > j ? i - j : j - i); best = copy[j] }
			if (best == "") best = "unattributed"
			count[best]++
		}
		for (c in count) printf "%s\tmain loop\t%s\t1048576\t%d\t-\n", variant, c, count[c]
	}' "$dir/annotate.txt"
}

measure() {
	snowghost=$1 compiler=$2 name=$3 results=$4
	script=$snowghost/research/investigations/style/scan-param.sh
	mkdir -p "$results"
	for variant in plain regcall preserve_none; do
		case $variant in
		plain) cc= ;;
		regcall) cc=x86_regcallcc ;;
		preserve_none) cc=preserve_nonecc ;;
		esac
		target=$results/$name-$variant
		rm -rf "$target" "$snowghost/build"
		mkdir -p "$target/rewritten-ir"
		echo "::group::$name $variant"
		status=0
		(
			cd "$snowghost" || exit 1
			WF_CC=$cc WF_CC_LOG=$target/rewritten-signatures.txt WF_CC_KEEP=$target/rewritten-ir \
				WHITEFOOTC=$compiler sh "$script" witness
		) >"$target/witness.log" 2>&1 || status=$?
		cat "$target/witness.log"
		echo "$name $variant: scan-param.sh witness exit $status"
		echo "$status" >"$target/status.txt"
		if [ -n "$cc" ]; then
			echo "-- $name $variant: the rewritten signatures"
			cat "$target/rewritten-signatures.txt" 2>/dev/null || echo "(the wrapper rewrote nothing)"
		fi
		out=$snowghost/build/scan-param/witness
		for file in calls.txt annotate.txt signatures.txt symbols.txt callgrind.log argument_words.out.gz argument_words.ll.gz argument_words.s; do
			cp "$out/$file" "$target/" 2>/dev/null || true
		done
		if [ -f "$target/calls.txt" ] && [ -f "$target/annotate.txt" ]; then
			summarize "$target" "$variant" >"$target/summary.tsv"
		else
			: >"$target/summary.tsv"
		fi
		echo "::endgroup::"
	done
}

report() {
	results=$1
	shift
	for variant in plain regcall preserve_none; do
		echo "=== $variant (instructions per call: exclusive / inclusive; the main loop's rows count the instructions run once per call of step)"
		for name in "$@"; do
			file=$results/$name-$variant/summary.tsv
			[ -s "$file" ] || echo "($name-$variant has no summary: status $(cat "$results/$name-$variant/status.txt" 2>/dev/null || echo missing))"
		done
		for name in "$@"; do
			if [ -s "$results/$name-$variant/summary.tsv" ]; then
				sed "s/^/$name\t/" "$results/$name-$variant/summary.tsv"
			fi
		done | awk -F'\t' -v order="$*" '
		{
			name = $1; fn = $3; copy = $4
			functions[fn] = 1
			calls[name, fn, copy] = $5; excl[name, fn, copy] = $6; incl[name, fn, copy] = $7
		}
		END {
			n = split(order, names, " ")
			split("step compound test main loop", dummy, " ")
			list[1] = "step"; list[2] = "compound"; list[3] = "test"; list[4] = "main loop"
			printf "%-8s %-10s | %10s %10s %10s | %10s %10s %10s | %10s %10s\n", "build", "function", "calls with", "ex/call", "incl/call", "calls w/o", "ex/call", "incl/call", "diff ex", "diff incl"
			for (i = 1; i <= n; i++) for (k = 1; k <= 4; k++) {
				name = names[i]; fn = list[k]
				if (!(fn in functions)) continue
				we = excl[name, fn, "with"]; wo = excl[name, fn, "without"]
				wi = incl[name, fn, "with"]; woi = incl[name, fn, "without"]
				de = (we ~ /^[0-9.]+$/ && wo ~ /^[0-9.]+$/) ? sprintf("%.2f", we - wo) : "-"
				di = (wi ~ /^[0-9.]+$/ && woi ~ /^[0-9.]+$/) ? sprintf("%.2f", wi - woi) : "-"
				printf "%-8s %-10s | %10s %10s %10s | %10s %10s %10s | %10s %10s\n", name, fn, calls[name, fn, "with"], we, wi, calls[name, fn, "without"], wo, woi, de, di
			}
		}'
	done
}

case ${1:-} in
wrapper) wrapper "$2" "$3" ;;
measure) measure "$2" "$3" "$4" "$5" ;;
report)
	shift
	report "$@"
	;;
*)
	echo "usage: $0 wrapper REAL_CLANG OUT | measure SNOWGHOST COMPILER NAME RESULTS | report RESULTS NAME..." >&2
	exit 2
	;;
esac
