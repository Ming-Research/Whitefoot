# Measure spelling

How a program writes a storage's measures and a window's named parts.

### decision measures-are-readonly-field-reads
A storage's length, capacity and ring head are read as its readonly fields `r.len`, `r.cap` and `r.head`, which no program can assign, with no reader form beside them.
- replaced: Reading measures with `len_of(place)`, `cap_of(place)`, `room_of(place)` and `head_of(place)` was on main; the owner changed it, because the measures are fields of a declared type, and a reader should not have to learn a second way to read a member.

### decision measure-names-are-not-reserved
`len`, `cap`, `head` and the four window part names are chosen by the type of the place they follow, so any binding, parameter, field or label may use them.
- replaced: Reserving `len`, `cap`, `room`, `head`, `next`, `last`, `filled` and `free` from every declaration role was on main; the owner changed it, because the member a selection names follows from the place's type, and the reservation forbade names such as a list node's `next`.

### decision window-parts-are-row-vocabulary
A window's four named parts are written as members of the storage place, only in declared effect rows and the overlap judgment, and no program refers to, reads or writes a part.
- rejected: an agent proposed a private part vocabulary for the built-in operations, with a measure written as a call; the owner rejected it, because the parts must be words an ordinary function declaring the same behavior can also write in its row.
- rejected: an agent proposed parts as ordinary places a program could name, read and borrow; the owner rejected it, because a part is a region of a window, not a place that holds a value.

### decision the-language-names-two-spellings
The length of a range reference and the four window part names are named by the language and by no declaration.
- rejected: an agent proposed giving a range reference a declared type and each window part a declared member; the owner rejected it, because neither has anything to be a field of, so that would invent a nominal for a path step and a place for each region.

### decision measures-are-read-not-borrowed
Borrowing a measure that has no field declaration is refused at the complete path under MSR-1, and the measure is read by value.
- rejected: an agent proposed admitting the borrow by making a temporary reference target; the owner rejected it, because reading a measure supplies a value, not a declared field that can be borrowed.
