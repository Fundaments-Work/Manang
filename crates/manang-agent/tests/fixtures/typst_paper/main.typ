#set document(title: "Landauer revisited"); #set heading(numbering: "1.")

= Introduction <sec:intro>
Information erasure carries a thermodynamic cost @landauer1961. The cost is small in absolute terms, but it matters for dense computing hardware because every bit of heat must eventually leave the chip, and chips keep getting denser. We make this precise in @sec:model.

= Model <sec:model>
Let $S = k_B ln 2$ be the entropy change per erased bit. The bound is $E >= k_B T ln 2$.

#bibliography("refs.bib")
