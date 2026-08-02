// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

export interface SelectOption {
  value: string;
  label: string;
  /** Secondary text shown dimmed on the right — a count, a hint. */
  hint?: string;
}

interface Props {
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  label: string;
  width?: number;
}

/**
 * Drop-down replacement for `<select>`.
 *
 * A native select's popup is drawn by GTK, outside the page, so no stylesheet
 * can reach it: on this dark UI it opened as a white system list. This one is
 * plain DOM, which means it can be themed — and it has to bring its own
 * keyboard handling, since that is what the native widget was providing.
 */
export default function Select({
  value,
  options,
  onChange,
  label,
  width = 180,
}: Props) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [dropUp, setDropUp] = useState(false);

  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const search = useRef({ text: "", at: 0 });
  const id = useId();

  const selected = options.findIndex((option) => option.value === value);
  const current = options[selected];

  // Close when the click lands anywhere else.
  useEffect(() => {
    if (!open) return;

    const onPointerDown = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  // Open upwards when there is no room below.
  useLayoutEffect(() => {
    if (!open || !rootRef.current) return;

    const box = rootRef.current.getBoundingClientRect();
    setDropUp(window.innerHeight - box.bottom < 260 && box.top > 260);
    setActive(selected < 0 ? 0 : selected);
  }, [open, selected]);

  // Keep the highlighted row in view for both keyboard and mouse.
  useEffect(() => {
    if (!open) return;
    listRef.current
      ?.querySelector<HTMLElement>('[data-active="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  function close(refocus = true) {
    setOpen(false);
    // Picking an option with the mouse used to leave focus on a `<div>` that
    // was about to be removed, which dropped keyboard navigation entirely.
    if (refocus) triggerRef.current?.focus();
  }

  function commit(index: number) {
    const option = options[index];
    if (option) onChange(option.value);
    close();
  }

  function onKeyDown(e: React.KeyboardEvent) {
    switch (e.key) {
      case "Enter":
      case " ":
        e.preventDefault();
        if (open) commit(active);
        else setOpen(true);
        return;
      case "Escape":
        if (open) {
          e.preventDefault();
          e.stopPropagation();
          close();
        }
        return;
      case "ArrowDown":
      case "ArrowUp": {
        e.preventDefault();
        if (!open) {
          setOpen(true);
          return;
        }
        const step = e.key === "ArrowDown" ? 1 : -1;
        setActive((i) => Math.min(options.length - 1, Math.max(0, i + step)));
        return;
      }
      case "Home":
        if (open) {
          e.preventDefault();
          setActive(0);
        }
        return;
      case "End":
        if (open) {
          e.preventDefault();
          setActive(options.length - 1);
        }
        return;
      case "Tab":
        setOpen(false);
        return;
    }

    // Type-ahead: typing jumps to the first match, the way a native select
    // does. Keystrokes within a second build up a longer prefix.
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
      const now = Date.now();
      search.current.text =
        now - search.current.at > 1000 ? e.key : search.current.text + e.key;
      search.current.at = now;

      const needle = search.current.text.toLowerCase();
      const hit = options.findIndex((o) => o.label.toLowerCase().startsWith(needle));
      if (hit >= 0) {
        setActive(hit);
        if (!open) commit(hit);
      }
    }
  }

  return (
    <div
      className="select"
      ref={rootRef}
      style={{ width }}
      // Tabbing away, or a controller moving focus out, has to close the menu
      // too — otherwise it stayed open, floating over whatever came next.
      onBlur={(e) => {
        if (open && !e.currentTarget.contains(e.relatedTarget as Node)) {
          close(false);
        }
      }}
    >
      <button
        type="button"
        ref={triggerRef}
        className={`select-trigger${open ? " open" : ""}`}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={onKeyDown}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-activedescendant={open ? `${id}-option-${active}` : undefined}
        aria-label={label}
      >
        {/* Falling back to `options[0]` showed a value that was not selected
            whenever the current one was filtered out of the list. */}
        <span className="select-value">{current?.label ?? "—"}</span>
        <ChevronDown size={15} className="select-arrow" />
      </button>

      {open && (
        <div
          className={`select-menu${dropUp ? " up" : ""}`}
          id={`${id}-list`}
          role="listbox"
          aria-label={label}
          ref={listRef}
        >
          {options.map((option, index) => (
            <div
              key={option.value}
              id={`${id}-option-${index}`}
              role="option"
              aria-selected={option.value === value}
              data-active={index === active}
              className={`select-option${index === active ? " active" : ""}${
                option.value === value ? " selected" : ""
              }`}
              onMouseEnter={() => setActive(index)}
              onClick={() => commit(index)}
            >
              <Check size={13} className="select-check" />
              <span className="select-label">{option.label}</span>
              {option.hint && <span className="select-hint">{option.hint}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
