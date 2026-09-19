# Rewriting a component in React

Read this before touching anything under `src/react/`. It is short on
purpose; everything in it exists because the Svelte version did it and the
React version has to keep doing it.

## The shape

```tsx
import { useStore } from "../store/useStore";
import { chat, chatSignal } from "../store/chat";

interface Props {
    label: string;
    onpick: (id: string) => void;
}

export default function Thing({ label, onpick }: Props) {
    const state = useStore(chatSignal, chat);
    return <button onClick={() => onpick(state.view)}>{label}</button>;
}
```

- **One component per file**, default export, named after the file.
- **Props are a typed `interface`**, destructured in the signature.
- **Callback props keep their Svelte names**: `onpick`, `onsave`, `ontest`.
  They are already the vocabulary of the panes and the call sites read the
  same either way. Do not rename them to `handlePick`.
- **DOM handlers use React's names**: `onClick`, `onChange`, `onKeyDown`.

## State

The stores are plain observable objects, not hooks and not context. Read one
with `useStore(signal, store)`; it returns the store and re-renders on change.

**Reassign, never mutate.** `items.push(x)` is invisible to the proxy behind
the stores; `items = [...items, x]` is not. This already bit the chat feed
during the store conversion -- `messages.push` compiled, typechecked, and
silently stopped drawing.

Component-local state is `useState` as normal.

## Translating the runes

| Svelte | React |
|---|---|
| `let x = $state(0)` | `const [x, setX] = useState(0)` |
| `const y = $derived(a + b)` | `const y = a + b` (plain expression) |
| `const y = $derived.by(() => {...})` | `const y = useMemo(() => {...}, [deps])` |
| `$effect(() => {...})` | `useEffect(() => {...}, [deps])` |
| `onMount(() => {...})` | `useEffect(() => {...}, [])` |
| `{#if a}` / `{:else}` | `{a ? <X/> : <Y/>}` |
| `{#each xs as x (x.id)}` | `{xs.map(x => <X key={x.id}/>)}` |
| `bind:value={v}` | `value={v} onChange={e => setV(e.target.value)}` |
| `class:active={on}` | `className={on ? "row active" : "row"}` |
| `{@const x = ...}` | a `const` above the `return`, or a small component |

**`$derived` is usually not `useMemo`.** A plain expression recomputes on
render, which is what `$derived` did. Reach for `useMemo` when the work is
real -- filtering 58 tools, parsing markdown -- not for `a + b`.

## Styles

**Do not write new CSS.** The design system is 67 variables in
`src/styles.css`, plus a shared settings vocabulary (`.row`, `.entry`,
`.tiny`, `.risk`, `.result`, `.disclosure`, `.snippet`) already promoted to
global. Use the class names the Svelte component used; they still exist.

If a Svelte component had a `<style>` block, its rules were scoped to it.
Move those rules into `src/react/styles/<component>.css` and import it from
the component. Keep the selectors identical so nothing else shifts.

**Never invent a colour or a spacing value.** Everything comes from a `var(--…)`.

## Tests

Every rewritten component gets a test beside it, `<Name>.test.tsx`, using
`@testing-library/react`. Test what a user can observe -- text on screen, what
a click does -- not internal state.

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
```

## What not to do

- No new dependencies. React, react-dom and the Tauri API are what there is.
- No Tailwind yet. That is a later, separate decision.
- No renaming of files, props or CSS classes beyond what is above.
- No "improvements" to behaviour while translating. If something looks wrong,
  say so in your report and leave it working as it was.
- Do not touch `src/lib/api.ts`, `src/lib/markdown.ts` or `src/lib/reactor.ts`.
  They have no framework in them and are used as-is from React.
