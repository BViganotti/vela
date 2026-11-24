#!/usr/bin/env python3
"""Playwright end-to-end test for the Vela Studio.

Starts the compiled Studio (no browser auto-open), drives Chromium:
load a module, run it, set an instruction breakpoint, step, rewind via the
timeline, and run the spec dashboard. Exit code 0 on success.

The page runs under a strict CSP without 'unsafe-eval', so this test only uses
DOM interactions and reads rendered text from Python.
"""

import os
import re
import subprocess
import sys
import time

from playwright.sync_api import sync_playwright


def wait_text(page, selector, predicate, timeout=20.0, label=""):
    deadline = time.time() + timeout
    last = ""
    while time.time() < deadline:
        last = page.inner_text(selector).strip()
        if predicate(last):
            return last
        time.sleep(0.05)
    raise AssertionError(f"timed out waiting for {label or selector}: last={last!r}")


def step_number(page) -> int:
    text = page.inner_text("#step-badge").strip()
    return int(text.split()[1])


def main() -> int:
    binary = os.environ.get("VELA_STUDIO_BIN", "./target/release/vela-studio")
    if not os.path.exists(binary):
        binary = "./target/debug/vela-studio"
    env = dict(os.environ)
    env["VELA_NO_OPEN"] = "1"
    proc = subprocess.Popen(
        [binary],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env=env,
    )
    url = None
    try:
        deadline = time.time() + 10
        while time.time() < deadline:
            line = proc.stdout.readline()
            if not line:
                break
            m = re.search(r"VELA_STUDIO_URL=(\S+)", line)
            if m:
                url = m.group(1)
                break
        if not url:
            print("studio did not print a URL", file=sys.stderr)
            return 1

        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page(viewport={"width": 1440, "height": 1000})
            page.goto(url, wait_until="domcontentloaded")
            wait_text(page, "#status-pill", lambda t: t.lower() == "ready")
            print("studio opened:", url)

            # Run the module to completion.
            page.click("#btn-run")
            wait_text(page, "#status-pill", lambda t: t.lower() == "finished", timeout=30)
            total = step_number(page)
            assert total > 50, f"expected a real workload, got {total} steps"
            print("ran module to completion:", total, "steps")

            # Restart and set an instruction breakpoint by clicking the disassembly.
            page.click("#btn-restart")
            wait_text(page, "#step-badge", lambda t: t == "step 0")
            page.click('.disasm-row[data-offset="5"]')
            page.wait_for_selector("#bp-list li[data-index='0']", timeout=5000)
            print("instruction breakpoint set from disassembly")

            page.click("#btn-run")
            wait_text(page, "#status-pill", lambda t: t.lower() == "paused", timeout=20)
            paused = step_number(page)
            assert 0 < paused <= total, f"unexpected breakpoint step {paused}"
            print("breakpoint paused at step", paused)

            # Step forward, then reverse step.
            page.click("#btn-step")
            wait_text(page, "#step-badge", lambda t: t == f"step {paused + 1}")
            page.click("#btn-back")
            wait_text(page, "#step-badge", lambda t: t == f"step {paused}")
            print("forward and reverse stepping match")

            # Rewind the timeline to the beginning using the keyboard
            # (Home sets a range input to its minimum).
            page.locator("#timeline").focus()
            page.keyboard.press("Home")
            wait_text(page, "#step-badge", lambda t: t == "step 0", timeout=10)
            print("timeline rewind reached step 0")

            # Keyboard shortcuts: step forward with Space, back with B.
            page.click("#step-badge")
            page.keyboard.press("Space")
            wait_text(page, "#step-badge", lambda t: t == "step 1")
            page.keyboard.press("b")
            wait_text(page, "#step-badge", lambda t: t == "step 0")
            print("keyboard shortcuts work")

            # Spec dashboard.
            page.click("#btn-spec")
            wait_text(
                page,
                "#spec-summary",
                lambda t: "/41" in t and "assertions passed" in t,
                timeout=15,
            )
            summary = page.inner_text("#spec-summary").strip()
            assert "41/41" in summary, f"unexpected spec summary: {summary}"
            print("spec dashboard:", summary)

            page.screenshot(path="studio-tests/studio.png", full_page=True)
            browser.close()
        return 0
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


if __name__ == "__main__":
    sys.exit(main())
