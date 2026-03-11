# Parallel Variant Review

This example describes the intended isolated comparison flow for multiple solution variants.

## Goal

Run multiple candidate plans in isolated variant workspaces and compare the results before promotion.

## Current status

The codebase already contains variant primitives and simulation/promotion paths. This document describes the intended operator workflow rather than a polished public UX.

## Conceptual flow

1. Create a plan set.
2. Create multiple plan variants.
3. Simulate each variant in isolation.
4. Compare outputs, diffs, and review findings.
5. Promote the chosen variant.

## Why variants matter

Parallel variants let CURD act like an agent backend without touching the original workspace during exploration:

- each variant gets isolated state
- reviews are attached to variant outputs
- promotion is explicit

## Next UX direction

The longer-term human-facing form should be named `.curd` scripts that compile into variant-aware plan artifacts rather than hand-authored `plan.json`.
