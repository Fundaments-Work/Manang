#set document(title: "{{title_string}}")
#set page(paper: "a4", numbering: "1", margin: (x: 3cm, top: 3cm, bottom: 2.5cm))
#set text(size: 11pt)
#set heading(numbering: "1.1")

#text(size: 14pt, fill: rgb("#666666"))[Chapter 1]
#v(0.2em)
#text(size: 24pt, weight: "bold")[{{title}}]
#v(1em)

= Chapter Overview

This chapter investigates the foundational principles and theoretical background underpinning this dissertation. We examine existing literature, focusing on the seminal contributions of @knuth1984.

= Background and Foundations

== Historical Context

The evolution of structured documentation reflects ongoing efforts to bridge formal specifications with human readability.

== Formal Formulation

Consider an open system defined over state space $Omega$:

$ d/d t rho(t) = -i [H, rho(t)] + cal(D)(rho(t)) $

= Experimental Design

The methodology leverages deterministic evaluation pipelines to ensure reproducibility across experimental trials.

= Summary

The formulations presented in this chapter establish the baseline metrics utilized throughout the remainder of this thesis.

#bibliography("references.bib")
