# The one row of raw samples the core-count sweep writes for a recorded pass,
# sourced by linux-net-bench.sh and by echo-scale-summary-check.sh, so that the
# writer and the reader of the row (echo-scale-summary.sh) are checked against
# each other on the same twelve columns:
#
#   line cores pass rt_per_s p50_us p99_us connect_us client_busy_max_pct
#   drivers server_exit load_exit server_cpus
#
# A column with nothing to say holds a dash.
scale_row() {
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$@"
}
