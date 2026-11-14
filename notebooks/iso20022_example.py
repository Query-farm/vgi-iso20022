"""Charts and authored-fixture integrity for the ISO 20022 examples."""

import hashlib
import json
import re
from decimal import Decimal
from pathlib import Path
from xml.etree import ElementTree

import matplotlib.pyplot as plt
from vgi_notebooks import finish

SOURCE = "Synthetic payment messages · released VGI ISO 20022 v0.4.0 · local parsing"


def validate_amount(value, *, maximum="1000"):
    """Accept a bounded decimal string before interpolating into visible SQL."""
    if not isinstance(value, str) or not re.fullmatch(
        r"[0-9]{1,4}(?:[.][0-9]{1,2})?", value
    ):
        raise ValueError(
            "Use a nonnegative decimal string with at most two decimal places."
        )
    if not Decimal(0) <= Decimal(value) <= Decimal(maximum):
        raise ValueError(f"The amount must be between 0 and {maximum}.")


def verify_statements():
    """Verify authored bytes and the one-statement-per-file release constraint."""
    manifest = json.loads(Path("fixtures/manifest.json").read_text())
    for name, digest in manifest["files"].items():
        payload = Path("fixtures", name).read_bytes()
        assert hashlib.sha256(payload).hexdigest() == digest
        root = ElementTree.fromstring(payload)
        assert sum(el.tag.rsplit("}", 1)[-1] == "Stmt" for el in root.iter()) == 1
    return hashlib.sha256(
        json.dumps(manifest["files"], sort_keys=True).encode()
    ).hexdigest()


def plot_grain(grain):
    """Keep independent currency units and the naive repeated-amount comparison."""
    fig, axes = plt.subplots(1, len(grain), figsize=(11.8, 5.8), squeeze=False)
    for ax, row in zip(axes.flat, grain.itertuples(), strict=True):
        values = [float(row.entry_net), float(row.naive_detail_net)]
        bars = ax.barh([1, 0], values, color=["#247F70", "#C77842"], height=0.43)
        ax.bar_label(bars, labels=[f"{v:,.2f}" for v in values], padding=6, fontsize=12)
        ax.set_yticks([1, 0], ["Entry grain", "Detail grain"])
        ax.set(
            title=f"{row.stmt_id} · {row.ccy}",
            xlabel=f"Signed booked movement ({row.ccy})",
        )
        ax.axvline(0, color="#77877C", lw=0.8)
        ax.set_ylim(-0.55, 1.6)
        ax.margins(x=0.32)
        ax.grid(axis="x")
    finish(
        fig,
        "Extra detail rows repeat the entry amount",
        "Each currency has its own scale. The second bar intentionally uses the wrong row grain.",
        SOURCE + " · statement date 02 January 2026",
        name="01-entry-grain",
    )


def plot_reconciliation(result, minimum):
    """Compare implied closes to the unchanged reported close without mixing currencies."""
    fig, axes = plt.subplots(1, len(result), figsize=(11.8, 6.0), squeeze=False)
    for ax, row in zip(axes.flat, result.itertuples(), strict=True):
        values = [float(row.full_implied_close), float(row.selected_implied_close)]
        bars = ax.barh([1, 0], values, color=["#247F70", "#C77842"], height=0.43)
        ax.bar_label(bars, labels=[f"{v:,.2f}" for v in values], padding=6, fontsize=12)
        ax.axvline(float(row.closing_balance), color="#343D35", ls="--", lw=1.2)
        ax.text(
            0.02,
            1.08,
            f"Reported close {row.closing_balance:,.2f}",
            transform=ax.transAxes,
            fontsize=10,
        )
        ax.set_yticks([1, 0], ["All booked", "Selected"])
        ax.set(
            xlabel=f"Opening + included movement ({row.ccy})",
            title=f"{row.stmt_id} · minimum {Decimal(minimum):.2f} {row.ccy}",
        )
        ax.set_ylim(-0.6, 1.65)
        ax.set_xlim(0, max(*values, float(row.closing_balance)) * 1.35)
        ax.grid(axis="x")
    finish(
        fig,
        "The full statement reconciles; a filtered subset may not",
        "The dashed line is the reported close. Excluding entries changes the comparison population.",
        SOURCE + " · statement date 02 January 2026 · independent currency scales",
        name="02-closing-balances",
    )


def verify_migration():
    """Verify every authored payment/status message against its manifest."""
    root = Path("fixtures/migration")
    manifest = json.loads((root / "manifest.json").read_text())
    for name, digest in manifest["files"].items():
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == digest
    return hashlib.sha256(
        json.dumps(manifest["files"], sort_keys=True).encode()
    ).hexdigest()


def plot_migration(deltas, population, band):
    """Plot only comparable EUR deltas; keep all six identities in a count panel."""
    from matplotlib.ticker import MaxNLocator

    fig, (ax, scope) = plt.subplots(
        1, 2, figsize=(12.4, 6.2), gridspec_kw={"width_ratios": [1.3, 1]}
    )
    y = list(range(len(deltas)))
    values = [float(value) for value in deltas.delta]
    colors = ["#247F70" if value == 0 else "#C77842" for value in values]
    ax.barh(y, values, color=colors, height=0.42)
    ax.scatter(values, y, color=colors, s=50, zorder=3)
    for pos, value in zip(y, values, strict=True):
        ax.annotate(
            f"{value:+.2f} EUR",
            (value, pos),
            xytext=(7, 0),
            textcoords="offset points",
            va="center",
            fontsize=11,
        )
    ax.axvline(float(band), color="#596E63", ls="--", lw=1)
    ax.set_yticks(y, deltas.payment_id)
    ax.set(
        xlabel="MX − MT settlement amount (EUR)",
        title=f"Comparable pairs · display band {Decimal(band):.2f} EUR",
        xlim=(-0.07, 2.8),
        ylim=(-0.65, len(y) - 0.3),
    )
    ax.invert_yaxis()
    ax.grid(axis="x")
    labels = {
        "comparable": "Same currency",
        "currency differs": "Currency differs",
        "MT only": "MT only",
        "MX only": "MX only",
    }
    bars = scope.barh(
        range(len(population)),
        population.identities,
        height=0.42,
        color=["#247F70", "#C77842", "#81948B", "#A0ACA5"],
    )
    scope.set_yticks(range(len(population)), [labels[s] for s in population.pair_state])
    scope.bar_label(bars, padding=6, fmt="%d")
    scope.set(
        xlim=(0, 3.8),
        xlabel="Authored payment identities",
        title="All six identities remain visible",
    )
    scope.invert_yaxis()
    scope.xaxis.set_major_locator(MaxNLocator(integer=True))
    scope.grid(axis="x")
    finish(
        fig,
        "An amount comparison needs a counterpart and the same currency",
        "Zero is exact equality. The dashed display band groups small differences without erasing them.",
        SOURCE
        + " · six identities, five MT messages, five MX messages and three status reports",
        name="03-migration-differences",
    )
