"""
test_vessel.py
==============
Level 2 + Level 4 integration test suite for the Vessel compiler.

Run with:
    pytest tests/test_vessel.py -v

Or directly:
    python3 tests/test_vessel.py

Requires:
    - vesselc binary at ../target/release/vesselc (build with cargo build --release)
    - z3-solver (pip install z3-solver)
    - Python 3.10+
"""

import subprocess
import sys
import math
import os
import importlib.util
import tempfile
import pytest

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

HERE     = os.path.dirname(os.path.abspath(__file__))
COMPILER = os.path.join(HERE, "..", "target", "release", "vesselc")
PASS_DIR = os.path.join(HERE, "pass")
FAIL_DIR = os.path.join(HERE, "fail")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def compile_vessel(path: str, target: str = "python") -> tuple[int, str, str]:
    """Run vesselc on `path`, return (exit_code, stdout, stderr)."""
    result = subprocess.run(
        [COMPILER, path, "--target", target],
        capture_output=True, text=True
    )
    return result.returncode, result.stdout, result.stderr


def compile_and_run(vessel_src: str) -> tuple[str, str]:
    """
    Write vessel_src to a temp file, compile to Python, execute the Python,
    return (python_stdout, python_stderr).
    """
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(vessel_src)
        vpath = f.name
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        pypath = f.name

    try:
        rc, out, err = compile_vessel(vpath, "python")
        assert rc == 0, f"Compile failed: {err}"
        with open(pypath, "w") as f:
            f.write(out)
        result = subprocess.run(
            [sys.executable, pypath],
            capture_output=True, text=True
        )
        return result.stdout, result.stderr
    finally:
        os.unlink(vpath)
        os.unlink(pypath)


# ---------------------------------------------------------------------------
# Level 2A — Pass programs: compiler must succeed (exit 0)
# ---------------------------------------------------------------------------

PASS_FILES = [f for f in os.listdir(PASS_DIR) if f.endswith(".vessel")]
PASS_FILES.sort()

@pytest.mark.parametrize("fname", PASS_FILES)
def test_pass_compiles(fname):
    """Every program in tests/pass/ must compile without error."""
    path = os.path.join(PASS_DIR, fname)
    rc, out, err = compile_vessel(path)
    assert rc == 0, f"{fname} failed to compile:\n{err}"
    assert len(out) > 0, f"{fname} produced empty output"


@pytest.mark.parametrize("fname", PASS_FILES)
def test_pass_emits_vessel_network(fname):
    """Every compiled pass program must contain VesselNetwork."""
    path = os.path.join(PASS_DIR, fname)
    rc, out, _ = compile_vessel(path)
    assert rc == 0
    assert "VesselNetwork" in out, f"{fname}: VesselNetwork not in output"


@pytest.mark.parametrize("fname", PASS_FILES)
def test_pass_emits_phi_rebalance(fname):
    """Every compiled pass program must contain phi_rebalance."""
    path = os.path.join(PASS_DIR, fname)
    rc, out, _ = compile_vessel(path)
    assert rc == 0
    assert "phi_rebalance" in out, f"{fname}: phi_rebalance not in output"


@pytest.mark.parametrize("fname", PASS_FILES)
def test_pass_emits_th1(fname):
    """Every compiled pass program must emit T-H1 alpha derivation."""
    path = os.path.join(PASS_DIR, fname)
    rc, out, _ = compile_vessel(path)
    assert rc == 0
    assert "_th1(" in out, f"{fname}: T-H1 not in output"


@pytest.mark.parametrize("fname", PASS_FILES)
def test_pass_python_executes(fname):
    """Every compiled pass program must execute without Python errors."""
    path = os.path.join(PASS_DIR, fname)
    rc, out, _ = compile_vessel(path)
    assert rc == 0
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False) as f:
        f.write(out)
        pypath = f.name
    try:
        result = subprocess.run(
            [sys.executable, pypath],
            capture_output=True, text=True, timeout=10
        )
        assert result.returncode == 0, \
            f"{fname}: Python execution failed:\n{result.stderr}"
    finally:
        os.unlink(pypath)


# ---------------------------------------------------------------------------
# Level 2B — Fail programs: compiler must reject (exit != 0)
# ---------------------------------------------------------------------------

FAIL_FILES = [f for f in os.listdir(FAIL_DIR) if f.endswith(".vessel")]
FAIL_FILES.sort()

@pytest.mark.parametrize("fname", FAIL_FILES)
def test_fail_rejected(fname):
    """Every program in tests/fail/ must be rejected by the compiler."""
    path = os.path.join(FAIL_DIR, fname)
    rc, out, err = compile_vessel(path)
    assert rc != 0, \
        f"{fname} was ACCEPTED but should have been rejected.\nOutput:\n{out}"


# ---------------------------------------------------------------------------
# Level 2C — Specific semantic checks
# ---------------------------------------------------------------------------

def test_forbidden_zone_error_message():
    """Forbidden zone errors must mention 'forbidden zone' in the message."""
    src = "vessel Bad { kappa: 0.487, boundary: 0.487 }"
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        path = f.name
    try:
        rc, _, err = compile_vessel(path)
        assert rc != 0
        assert "forbidden zone" in err.lower() or "forbidden" in err.lower(), \
            f"Expected 'forbidden zone' in error, got:\n{err}"
    finally:
        os.unlink(path)


def test_sentient_vessel_accepted_in_zone():
    """Sentient vessels must be accepted in the forbidden zone."""
    src = "vessel H { kappa: 0.5, boundary: 0.5, sentient: true }"
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        path = f.name
    try:
        rc, out, err = compile_vessel(path)
        assert rc == 0, f"Sentient vessel rejected (should pass):\n{err}"
    finally:
        os.unlink(path)


def test_th1_invariant_at_runtime():
    """After compilation and execution, alpha == ln(kappa) for every vessel."""
    src = """
vessel A { kappa: 0.25, boundary: 0.25 }
vessel B { kappa: 0.75, boundary: 0.75 }
"""
    FORBIDDEN_LOW  = math.exp(-1)
    FORBIDDEN_HIGH = math.exp(-0.5)

    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        vpath = f.name
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False, dir="/tmp") as f:
        pypath = f.name

    try:
        rc, out, err = compile_vessel(vpath)
        assert rc == 0, f"Compile failed: {err}"

        # Append runtime assertions to the emitted code
        check = """
import math
net = VesselNetwork()
assert abs(net.A_alpha - math.log(net.A_kappa)) < 1e-9, \\
    f"T-H1 violated: A_alpha={net.A_alpha}, ln(A_kappa)={math.log(net.A_kappa)}"
assert abs(net.B_alpha - math.log(net.B_kappa)) < 1e-9, \\
    f"T-H1 violated: B_alpha={net.B_alpha}, ln(B_kappa)={math.log(net.B_kappa)}"
print("T-H1 invariant holds for all vessels.")
"""
        with open(pypath, "w") as f:
            f.write(out)
            f.write(check)

        result = subprocess.run(
            [sys.executable, pypath],
            capture_output=True, text=True, timeout=10
        )
        assert result.returncode == 0, f"T-H1 runtime check failed:\n{result.stderr}"
        assert "T-H1 invariant holds" in result.stdout
    finally:
        os.unlink(vpath)
        os.unlink(pypath)


def test_phi_rebalance_converges():
    """phi_rebalance must terminate and return a VesselNetwork instance."""
    src = """
vessel A { kappa: 0.25, boundary: 0.25 }
vessel B { kappa: 0.75, boundary: 0.75 }
"""
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        vpath = f.name
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False, dir="/tmp") as f:
        pypath = f.name

    try:
        rc, out, _ = compile_vessel(vpath)
        assert rc == 0

        check = """
net = VesselNetwork()
result = phi_rebalance(net)
assert isinstance(result, VesselNetwork), "phi_rebalance did not return VesselNetwork"
print("phi_rebalance converged OK")
"""
        with open(pypath, "w") as f:
            f.write(out)
            f.write(check)

        result = subprocess.run(
            [sys.executable, pypath],
            capture_output=True, text=True, timeout=10
        )
        assert result.returncode == 0, f"phi_rebalance failed:\n{result.stderr}"
        assert "phi_rebalance converged OK" in result.stdout
    finally:
        os.unlink(vpath)
        os.unlink(pypath)


def test_r7_assertion_fires_at_runtime():
    """A compiled function with R7 annotation must raise AssertionError when called with out-of-range arg."""
    src = """
vessel A { kappa: 0.2, boundary: 0.2 }
fn scale(x: <0.0, 0.35>) {
    return x
}
"""
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        vpath = f.name
    with tempfile.NamedTemporaryFile(suffix=".py", mode="w", delete=False, dir="/tmp") as f:
        pypath = f.name

    try:
        rc, out, _ = compile_vessel(vpath)
        assert rc == 0

        check = """
net = VesselNetwork()
try:
    scale(net, 0.9)   # 0.9 is outside <0.0, 0.35>
    print("FAIL: no assertion raised")
except AssertionError:
    print("R7 assertion fired correctly")
"""
        with open(pypath, "w") as f:
            f.write(out)
            f.write(check)

        result = subprocess.run(
            [sys.executable, pypath],
            capture_output=True, text=True, timeout=10
        )
        assert result.returncode == 0, f"Script failed unexpectedly:\n{result.stderr}"
        assert "R7 assertion fired correctly" in result.stdout
    finally:
        os.unlink(vpath)
        os.unlink(pypath)


def test_llvm_ir_stub_emitted():
    """--target llvm must produce LLVM IR with global kappa constants."""
    src = "vessel A { kappa: 0.2, boundary: 0.2 }"
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        path = f.name
    try:
        rc, out, err = compile_vessel(path, target="llvm")
        assert rc == 0, f"LLVM target failed:\n{err}"
        assert "@A_kappa" in out, "LLVM IR missing global kappa constant"
    finally:
        os.unlink(path)


def test_unknown_vessel_in_couple_error():
    """Coupling a vessel to an undeclared name must produce a compile error."""
    src = "vessel A { kappa: 0.2, boundary: 0.2 }\ncouple A to UNKNOWN { coupling: 0.5 }"
    with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
        f.write(src)
        path = f.name
    try:
        rc, _, err = compile_vessel(path)
        assert rc != 0, "Unknown vessel reference was not caught"
        assert "UNKNOWN" in err, f"Error should mention 'UNKNOWN', got:\n{err}"
    finally:
        os.unlink(path)


def test_forbidden_zone_exact_boundaries():
    """Values exactly at FORBIDDEN_LOW and FORBIDDEN_HIGH are NOT in the zone (open interval)."""
    FORBIDDEN_LOW  = math.exp(-1)
    FORBIDDEN_HIGH = math.exp(-0.5)

    # Just below FORBIDDEN_LOW — should pass
    src_low = f"vessel V {{ kappa: {FORBIDDEN_LOW - 0.001:.6f}, boundary: {FORBIDDEN_LOW - 0.001:.6f} }}"
    # Just above FORBIDDEN_HIGH — should pass
    src_high = f"vessel V {{ kappa: {FORBIDDEN_HIGH + 0.001:.6f}, boundary: {FORBIDDEN_HIGH + 0.001:.6f} }}"

    for src, label in [(src_low, "below FL"), (src_high, "above FH")]:
        with tempfile.NamedTemporaryFile(suffix=".vessel", mode="w", delete=False) as f:
            f.write(src)
            path = f.name
        try:
            rc, _, err = compile_vessel(path)
            assert rc == 0, f"Vessel {label} should pass but was rejected:\n{err}"
        finally:
            os.unlink(path)


# ---------------------------------------------------------------------------
# Level 4 — Z3 formal proofs (imported from proofs/vessel_proofs.py)
# ---------------------------------------------------------------------------

def test_z3_proofs_all_pass():
    """Run the Z3 proof suite and assert all 8 theorems are proved/witnessed."""
    proof_path = os.path.join(HERE, "..", "proofs", "vessel_proofs.py")
    result = subprocess.run(
        [sys.executable, proof_path],
        capture_output=True, text=True, timeout=60
    )
    assert result.returncode == 0, f"Proof script crashed:\n{result.stderr}"
    assert "ALL THEOREMS PROVED OR WITNESSED" in result.stdout, \
        f"Not all theorems passed:\n{result.stdout}"
    assert "Proved / Witnessed : 8" in result.stdout, \
        f"Expected 8 proved, got:\n{result.stdout}"


# ---------------------------------------------------------------------------
# Direct run (non-pytest)
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import traceback

    total = passed = failed = 0
    errors = []

    def run(name, fn):
        global total, passed, failed
        total += 1
        try:
            fn()
            print(f"  PASS  {name}")
            passed += 1
        except Exception as e:
            print(f"  FAIL  {name}")
            errors.append((name, traceback.format_exc()))
            failed += 1

    print("=" * 60)
    print("Vessel Compiler — Level 2 + Level 4 Test Suite")
    print("=" * 60)

    # Pass corpus
    print("\n--- Pass corpus ---")
    for fname in sorted(PASS_FILES):
        run(f"compiles({fname})",          lambda f=fname: test_pass_compiles(f))
        run(f"emits_network({fname})",     lambda f=fname: test_pass_emits_vessel_network(f))
        run(f"emits_phi({fname})",         lambda f=fname: test_pass_emits_phi_rebalance(f))
        run(f"emits_th1({fname})",         lambda f=fname: test_pass_emits_th1(f))
        run(f"py_executes({fname})",       lambda f=fname: test_pass_python_executes(f))

    # Fail corpus
    print("\n--- Fail corpus ---")
    for fname in sorted(FAIL_FILES):
        run(f"rejected({fname})",          lambda f=fname: test_fail_rejected(f))

    # Semantic checks
    print("\n--- Semantic checks ---")
    run("forbidden_zone_error_message",    test_forbidden_zone_error_message)
    run("sentient_accepted_in_zone",       test_sentient_vessel_accepted_in_zone)
    run("th1_invariant_at_runtime",        test_th1_invariant_at_runtime)
    run("phi_rebalance_converges",         test_phi_rebalance_converges)
    run("r7_assertion_fires",              test_r7_assertion_fires_at_runtime)
    run("llvm_ir_stub_emitted",            test_llvm_ir_stub_emitted)
    run("unknown_vessel_error",            test_unknown_vessel_in_couple_error)
    run("forbidden_exact_boundaries",      test_forbidden_zone_exact_boundaries)

    # Level 4
    print("\n--- Level 4: Z3 Formal Proofs ---")
    run("z3_all_8_theorems",               test_z3_proofs_all_pass)

    # Summary
    print("\n" + "=" * 60)
    print(f"  {passed}/{total} passed   {failed} failed")
    if errors:
        print("\nFailures:")
        for name, tb in errors:
            print(f"\n  [{name}]\n{tb}")
    else:
        print("  ALL TESTS PASSED.")
    print("=" * 60)
    sys.exit(0 if failed == 0 else 1)
