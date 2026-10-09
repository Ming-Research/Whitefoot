#ifndef WHITEFOOT_SCHED_ENTRY_H
#define WHITEFOOT_SCHED_ENTRY_H
#include "core.h"
/* Called before the ordinary program entry to validate process settings.
 * Workers start lazily at the first admitted compute offer. */
void wf__runtime_start(void);
void wf__sched_once(unsigned *state, void (*body)(void));
int wf__sched_setting(const char *name, unsigned long ceiling, unsigned long *value);
unsigned long wf__sched_helper_ceiling(void);
int wf__sched_lanes(void);
uint64_t wf__sched_split_work(void);
int wf__sched_demand_requests(void);
int wf__sched_report(char *buffer, size_t capacity);
#endif
