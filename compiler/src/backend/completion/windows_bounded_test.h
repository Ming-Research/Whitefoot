#ifndef WHITEFOOT_WINDOWS_BOUNDED_TEST_H
#define WHITEFOOT_WINDOWS_BOUNDED_TEST_H

/* What the Windows helper-route bounded-wait tests share: the check macro,
 * the one host operation of the root context, and the three steps every such
 * test takes around a bounded call -- wait until a helper has claimed the
 * operation, end the bound (fire the watch, or wait out the deadline), and
 * drive the root context to the operation's completion while checking that
 * the driver kept no cancellation registration or timer.
 *
 * Include it after `bridge.c`, `../ordinary_values.c` and
 * `../runtime_test_guard.h`, with WF_BOUNDED_TEST_NAME defined as the string
 * that begins the test's failure lines. No production source includes it;
 * retire it with the tests that do. */

#define CHECK(test) do { if (!(test)) { \
    fprintf(stderr, "%s:%d: %s\n", WF_BOUNDED_TEST_NAME, __LINE__, #test); \
    exit(1); \
} } while (0)

static int frame;

static wf_host_operation *operation(void) {
    return wf__context_operation();
}

static void finish_operation(int state) {
    CHECK(state == 1 || state == 2);
    if (state == 2 && wf__context_wait(operation(), &frame)) {
        /* This handwritten frame has now unwound to its service owner. */
        wf_context_adopt_wait(&wf_driver_root, &wf_context_root);
        wf_context_current = NULL;
        while (wf_context_root.record != NULL) {
            wf_driver_reap(&wf_driver_root);
            if (wf_context_root.record == NULL) break;
            uint64_t epoch = wf_completion_wake_epoch(wf_driver_root.runtime);
            if (!wf_driver_find_cancelled(&wf_driver_root)
                && !wf_context_harvest(&wf_driver_root) && wf_context_root.record != NULL)
                wf_bridge_park(epoch, wf_timer_wait_ms(&wf_driver_root));
        }
        CHECK(wf_run_take(&wf_driver_root) == &wf_context_root);
        CHECK(wf_run_take(&wf_driver_root) == NULL);
        wf_context_current = &wf_context_root;
    }
    CHECK(wf_driver_root.cancel_waits == NULL);
    CHECK(wf_context_root.timer_slot == 0);
    CHECK(wf_driver_root.timer_count == 0);
}

static void end_bound(int cancelled, wf_cancel_handle *source, const wf_deadline *deadline) {
    if (cancelled) {
        wf_context firing = {0};
        wf_host_operation call = {0};
        uint8_t result;
        firing.driver = &wf_driver_root;
        wf_context_current = &firing;
        CHECK(wf__body_cancel_fire_start(&result, source, &call) == 3);
        CHECK(wf__context_wait(&call, &frame) == 0);
        wf__body_cancel_fire_finish(&result, source, &call);
        wf_context_current = &wf_context_root;
    } else while (wf_file_monotonic_ns() < deadline->value.ticks) Sleep(1);
}

/* Wait until a helper has claimed the operation, so the bound ends a running
 * helper rather than a record still in its queue. */
static void await_helper_claim(void) {
    for (;;) {
        int executing = 0;
        wf_completion_wait_lock(&wf_bridge_adapter.queue_wait);
        for (size_t i = 0; i < wf_bridge_adapter.helper_slots; i++)
            executing |= wf_bridge_adapter.executing[i] == &operation()->record;
        wf_completion_wait_unlock(&wf_bridge_adapter.queue_wait);
        if (executing) break;
        CHECK(wf_bridge_record_state(&operation()->record) != WF_COMPLETION_DONE);
        Sleep(1);
    }
}

#endif
