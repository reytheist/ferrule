import { useEffect, useRef } from "react";
import {
  Bold,
  Code,
  Italic,
  Link2,
  List,
  ListOrdered,
  Quote,
  RemoveFormatting,
  Underline,
} from "lucide-react";

export interface RichEditorProps {
  mode: "rich" | "plain";
  onModeChange: (mode: "rich" | "plain") => void;
  bodyText: string;
  bodyHtml: string;
  onChange: (next: { text: string; html: string }) => void;
  placeholder?: string;
  /** Incrementing token to force-sync external insertions (snippets, availability, signatures). */
  syncNonce: number;
}

export function htmlToPlainText(html: string): string {
  const div = document.createElement("div");
  div.innerHTML = html;
  return (div.innerText ?? div.textContent ?? "").replace(/\r\n/g, "\n").trim();
}

export function plainTextToHtml(text: string): string {
  const escaped = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
  return escaped.replace(/\n/g, "<br>");
}

export function RichEditor({
  mode,
  onModeChange,
  bodyText,
  bodyHtml,
  onChange,
  placeholder = "Write your message…",
  syncNonce,
}: RichEditorProps) {
  const editableRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Sync external updates (snippet, availability, signature) into the contentEditable DOM.
  useEffect(() => {
    if (mode !== "rich" || !editableRef.current) return;
    if (editableRef.current.innerHTML !== bodyHtml) {
      editableRef.current.innerHTML = bodyHtml;
    }
  }, [syncNonce, mode, bodyHtml]);

  function exec(command: string, value?: string): void {
    editableRef.current?.focus();
    document.execCommand(command, false, value);
    emitFromEditable();
  }

  function insertLink(): void {
    const raw = window.prompt("Link URL (https:// or mailto:)", "https://");
    if (!raw) return;
    const trimmed = raw.trim();
    if (
      !trimmed.toLowerCase().startsWith("https://") &&
      !trimmed.toLowerCase().startsWith("http://") &&
      !trimmed.toLowerCase().startsWith("mailto:")
    ) {
      return;
    }
    exec("createLink", trimmed);
  }

  function emitFromEditable(): void {
    const el = editableRef.current;
    if (!el) return;
    const html = el.innerHTML;
    const text = (el.innerText ?? el.textContent ?? "").replace(/\r\n/g, "\n");
    onChange({ text, html });
  }

  function switchMode(next: "rich" | "plain"): void {
    if (next === mode) return;
    if (next === "plain") {
      const plain = bodyHtml ? htmlToPlainText(bodyHtml) : bodyText;
      onChange({ text: plain, html: plainTextToHtml(plain) });
    } else {
      const html = bodyText ? plainTextToHtml(bodyText) : bodyHtml;
      onChange({ text: bodyText, html });
    }
    onModeChange(next);
  }

  return (
    <div className="rich-editor">
      <div className="rich-toolbar" role="toolbar" aria-label="Formatting">
        {mode === "rich" && (
          <>
            <button
              type="button"
              className="rich-btn"
              title="Bold (Ctrl+B)"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("bold")}
            >
              <Bold />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Italic (Ctrl+I)"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("italic")}
            >
              <Italic />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Underline (Ctrl+U)"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("underline")}
            >
              <Underline />
            </button>
            <span className="rich-sep" />
            <button
              type="button"
              className="rich-btn"
              title="Bulleted list"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("insertUnorderedList")}
            >
              <List />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Numbered list"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("insertOrderedList")}
            >
              <ListOrdered />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Quote"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("formatBlock", "blockquote")}
            >
              <Quote />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Inline code"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("formatBlock", "pre")}
            >
              <Code />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Insert link"
              onMouseDown={(e) => e.preventDefault()}
              onClick={insertLink}
            >
              <Link2 />
            </button>
            <button
              type="button"
              className="rich-btn"
              title="Clear formatting"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => exec("removeFormat")}
            >
              <RemoveFormatting />
            </button>
          </>
        )}
        <span style={{ flex: 1 }} />
        <button
          type="button"
          className={`rich-mode-btn${mode === "rich" ? " active" : ""}`}
          onClick={() => switchMode("rich")}
        >
          Rich
        </button>
        <button
          type="button"
          className={`rich-mode-btn${mode === "plain" ? " active" : ""}`}
          onClick={() => switchMode("plain")}
        >
          Plain
        </button>
      </div>

      {mode === "rich" ? (
        <div
          ref={editableRef}
          className="rich-surface"
          contentEditable
          suppressContentEditableWarning
          role="textbox"
          aria-multiline="true"
          aria-label="Message body"
          data-placeholder={placeholder}
          onInput={emitFromEditable}
        />
      ) : (
        <textarea
          ref={textareaRef}
          className="textarea rich-plain"
          placeholder={placeholder}
          value={bodyText}
          onChange={(event) => {
            const text = event.target.value;
            onChange({ text, html: plainTextToHtml(text) });
          }}
        />
      )}
    </div>
  );
}
