# In-place update form

How a program replaces the value of an owned place with a function of its old value.

### decision update-is-an-assignment-of-a-call
Replacing an owned place's value by a function of its old value is an ordinary assignment whose right-hand side is a call taking that place by value; the operation has no name of its own.
- rejected: an agent proposed a named built-in update operation taking a reference to the place and the function to apply; the owner rejected it, because it would give the construct a second spelling beside the assignment, which shows which place is updated and by what.
