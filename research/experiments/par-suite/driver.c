/* Whole-process checksum observer; the generated source owns all sizes. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
extern uint64_t wf_suite_checksum(void);
extern int wf__floor_run(int, char **);
int wf__main_body(int argc, char **argv) {
    (void)argc;
    (void)argv;
    printf("%" PRIu64 "\n", wf_suite_checksum());
    return 0;
}
int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
