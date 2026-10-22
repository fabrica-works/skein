"""The capture's end of the program: its output's end awaited (lab-browser.md, 5.1)."""
import tempfile
import unittest
from look.capture import Program, ProgramFailure


class ProgramEnd(unittest.TestCase):
    def run_program(self, script, seconds):
        with tempfile.TemporaryFile() as log:
            program = Program(["/bin/sh", "-c", script], log, seconds)
            try:
                self.assertEqual(program.answer(), {"ready": {"browser": "stand-in"}})
                program.finish()
            finally:
                program.dispose()

    def test_a_process_ending_after_the_program_holds_its_output_a_moment(self):
        # A process of the program's namespace may close its output after the
        # program has exited 0: the capture awaits the end, it does not fail.
        self.run_program('echo \'{"ready":{"browser":"stand-in"}}\'; (sleep 0.3) & exit 0', 5)

    def test_output_held_past_the_deadline_fails(self):
        with self.assertRaisesRegex(ProgramFailure, "output pipe open"):
            self.run_program('echo \'{"ready":{"browser":"stand-in"}}\'; (sleep 2) & exit 0', 1)


if __name__ == "__main__":
    unittest.main()
