# Spec: design gate — owner session between packets and the data gate

Purpose: before anything is built, the owner verifies that **every workflow they care about is represented and properly ordered**, and locks the design language. Inputs: the packet set (stories) + a Penpot project structured in the four layers below. Every layer is **prototyped** — the owner gets interaction feel, not static frames.

## The four layers (dentconnex practice, generalized)

1. **Base screen** — ONE representative, dense screen containing the main elements. This is where direction and styling language are checked: dark vs light mode, typography, spacing. Colors and tokens are **linked library assets** — updating one updates every use; a design that needs cascade-editing across pages fails this layer structurally.
2. **Component gallery** — all components in one view, to inspect the vibe the application gives off.
3. **User flows** — every human-visible journey from the packet set, represented end to end and in the order the packets claim.
4. **Breakpoints** — select flows rendered at different widths (phone-first products: phone is the primary rendering, desktop the variant).

## Iteration mechanic

When direction is contested, add **option pages**: alternatives presented side by side for the owner to choose among — iterate by selection, not by serial rework.

## Exit criteria

Owner strikes/reorders workflows (feeding packet deltas), and signs off the design language. After sign-off, tokens are the contract the build styles against. A later loop-back from the data gate (new surfaces discovered) reopens layers 3–4 for those flows only; layer 1 reopens only if direction itself failed.
