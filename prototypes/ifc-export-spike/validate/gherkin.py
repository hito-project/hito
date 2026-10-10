"""Run the buildingSMART Validation Service's normative rules locally.

    python gherkin.py PATH/TO/ifc-gherkin-rules FILE.ifc

The rules (github.com/buildingSMART/ifc-gherkin-rules, MIT) are the same
behave features the online Validation Service runs. Their runner asks git for
each rule's version; outside a git checkout we report version 0 instead.
Prints every outcome above "executed/passed" and exits non-zero on errors.
"""

import collections
import os
import sys

rules_dir = os.path.abspath(sys.argv[1])
sys.path.insert(0, rules_dir)

import main as gherkin  # noqa: E402  (ifc-gherkin-rules/main.py)

gherkin.get_remote = lambda cwd: "local"
gherkin.get_commits = lambda cwd, feature_file: []

counts = collections.Counter()
errors = 0
for outcome in gherkin.run(sys.argv[2], execution_mode=gherkin.ExecutionMode.TESTING):
    if "feature_name" in outcome:
        counts["rules"] += 1
        continue
    if "protocol_errors" in outcome or "caught_exceptions" in outcome:
        # Rule-authoring checks of the test harness, not findings about the file.
        counts["harness notes"] += 1
        continue
    severity = str(outcome.get("severity"))
    counts[severity] += 1
    if severity in ("WARNING", "ERROR"):
        errors += severity == "ERROR"
        print(severity, outcome.get("feature"), outcome.get("outcome_code"), outcome.get("instance_id"),
              outcome.get("expected"), outcome.get("observed"))
print(dict(counts))
print("PASS" if errors == 0 else f"{errors} error(s)")
sys.exit(1 if errors else 0)
