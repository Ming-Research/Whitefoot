; Use the same entry-world binder as the formal compute fixtures.
define i64 @wf_suite_checksum() {
  %value = call i64 @wf_suite_entry()
  ret i64 %value
}
