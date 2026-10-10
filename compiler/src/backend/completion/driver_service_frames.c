/* Strong handwritten coroutine entries for driver_service_test.c. This
 * separate translation unit replaces the bridge's weak no-library entries
 * without a conditional path in the production bridge. Linked only by that
 * probe, including its sanitizer variants. */
#include "bridge.h"

extern void wf_driver_service_test_resume(void *frame);
extern int wf_driver_service_test_done(void *frame);

void wf__coro_resume(void *frame) { wf_driver_service_test_resume(frame); }
int wf__coro_done(void *frame) { return wf_driver_service_test_done(frame); }
void wf__coro_destroy(void *frame) { (void)frame; }
