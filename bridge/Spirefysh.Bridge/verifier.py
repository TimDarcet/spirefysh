#!/usr/bin/env python3
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from uuid import uuid4

import sts2_sim


root = Path(sys.argv[1])
env = sts2_sim.Batch(1)
for line in sys.stdin:
    request = None
    try:
        request = json.loads(line)
        differences = env.compare_transition(json.dumps(request["transition"]))
        if not differences:
            print(json.dumps({"transition_match": True}), flush=True)
            continue
        error = None
    except Exception as exception:
        differences = []
        error = f"{type(exception).__name__}: {exception}"
    folder = root / f"{datetime.now(timezone.utc):%Y%m%dT%H%M%S.%fZ}-{uuid4().hex[:8]}"
    folder.mkdir(parents=True)
    (folder / "transition.json").write_text(json.dumps(request, indent=2, sort_keys=True) + "\n")
    report = {"differences": differences, "error": error}
    (folder / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    warning = error or f"{len(differences)} simulator difference{'s' if len(differences) != 1 else ''}"
    print(json.dumps({"transition_match": False, "warning": warning, "path": str(folder)}), flush=True)
