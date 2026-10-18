---
title: Messy Markdown Test
author: Tester
---

# Heading 1

Some regular text with [bad link](javascript:alert(1)) and [data link](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==).

<script>alert("evil XSS")</script>
<img src="notfound.png" onerror="alert(2)" />
<span onpointerdown="evil()" onclick="evil2()">Clickable</span>

Here is an unbalanced math expression: $E = mc^2 and no closing dollar.
Another line with $$display math without closing.

```mermaid
invalid flowchart syntax here %% not a diagram
```

```python
def unclosed_code_fence():
    return True
