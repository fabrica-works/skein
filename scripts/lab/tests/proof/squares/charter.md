# Squares

The `ledger` tool reaches a small service. Call it with args `["task"]`: it prints a Python
function, `odd_squares`, that has one bug, and twelve numbers.

1. Write a program that defines `odd_squares` with its bug fixed and prints
   `odd_squares` of the twelve numbers. Run it without writing any file: pipe it to
   `python3 -` through a heredoc (`python3 - <<'EOF'` ... `EOF`).
2. Send the program: the `ledger` tool with args `["program"]` and the program's text as
   its stdin.
3. Send the result: args `["answer", "<the printed number>"]`. The tool prints `accepted`
   or `wrong`.

You are done when an answer is accepted. Edit no file: the work needs none.
