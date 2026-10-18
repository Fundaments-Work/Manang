#set document(title: "{{title_string}}")
#set page(paper: "a4", margin: (x: 2.5cm, top: 3cm, bottom: 2.5cm))
#set text(size: 11pt)

#grid(
  columns: (1fr, 1fr),
  align(left)[
    *Author Name* \
    Department of Science \
    University of Excellence \
    City, Country
  ],
  align(right)[
    #datetime.today().display("[month repr:long] [day], [year]")
  ]
)

#v(2em)

*Recipient Name* \
Hiring Committee / Editorial Office \
Institution Name \
Address Line \
City, Postcode

#v(1.5em)

*Subject: {{title}}*

#v(1em)

Dear Committee Members,

I am writing to formally submit our application regarding the position referenced above. With extensive background in scientific research and systems engineering, I welcome the opportunity to contribute to your ongoing initiatives.

During recent projects, our laboratory developed automated workflows that substantially reduced iteration times while maintaining rigorous verification standards. Our team places primary emphasis on clarity, reproducible methodology, and principled design.

Thank you for your time and consideration. I look forward to hearing from you.

#v(2em)

Sincerely,

#v(1.5em)

*Author Name* \
Senior Researcher
