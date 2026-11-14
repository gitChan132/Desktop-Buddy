import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import { playCroak } from "../croak";

/**
 * The default click action: a cozy chat with the frog. The backend pins the
 * persona and deflects anything that is not feelings-support.
 */
interface ChatMessage {
  who: "frog" | "you";
  text: string;
}

const GREETING: ChatMessage = {
  who: "frog",
  text: "*ribbit* Wanna talk about it? What's sitting on your chest right now?",
};

const OFFLINE: ChatMessage = {
  who: "frog",
  text: "*ribbit* My pond line is quiet right now. Tell me anyway — we can start with one slow breath.",
};

const MAX_INPUT = 400;
const MAX_SHOWN = 40;

interface ChatWithFrogProps {
  open: boolean;
  onClose: () => void;
}

export function ChatWithFrog({ open, onClose }: ChatWithFrogProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([GREETING]);
  const [text, setText] = useState("");
  const [pending, setPending] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Fresh, private conversation each time the chat is opened.
  useEffect(() => {
    if (!open) {
      setMessages([GREETING]);
      setText("");
      setPending(false);
      return;
    }
    const focus = setTimeout(() => inputRef.current?.focus(), 80);
    return () => clearTimeout(focus);
  }, [open]);

  // Keep the newest line in view.
  useEffect(() => {
    const node = scrollRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [messages, pending]);

  const send = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = text.trim();
    if (!trimmed || pending) return;

    const history = messages.map((message) => ({
      role: message.who === "you" ? "user" : "model",
      text: message.text,
    }));
    setMessages((current) =>
      [...current, { who: "you" as const, text: trimmed }].slice(-MAX_SHOWN),
    );
    setText("");
    setPending(true);
    playCroak();

    try {
      const reply = await invoke<string>("frog_chat", {
        history,
        message: trimmed,
      });
      setMessages((current) =>
        [...current, { who: "frog" as const, text: reply }].slice(-MAX_SHOWN),
      );
    } catch {
      setMessages((current) => [...current, OFFLINE].slice(-MAX_SHOWN));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-[130px] z-50 flex flex-col items-center gap-1.5 px-3">
      <AnimatePresence>
        {open && (
          <motion.div
            data-interactive
            initial={{ opacity: 0, y: 10, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 8, scale: 0.97 }}
            transition={{ type: "spring", stiffness: 320, damping: 26 }}
            className="pointer-events-auto pixel-panel w-[292px] px-3 py-2.5 text-emerald-950"
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
            }}
          >
            <div className="-mx-3 -mt-2.5 mb-2 flex items-center justify-between bg-[#0b3a1f] px-2 py-1">
              <span className="text-[14px] uppercase leading-none tracking-[0.12em] text-emerald-100">
                Talk to the frog
              </span>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close the chat"
                className="pixel-button px-1.5 py-0.5 text-[12px] uppercase leading-none text-emerald-900"
              >
                Close
              </button>
            </div>

            <div
              ref={scrollRef}
              className="flex max-h-[168px] flex-col gap-1.5 overflow-y-auto pr-0.5"
            >
              {messages.map((message, index) => (
                <p
                  key={`${index}-${message.who}`}
                  className={
                    message.who === "frog"
                      ? "self-start border-2 border-emerald-900/20 bg-emerald-100/70 px-1.5 py-1 text-[16px] leading-[1.15]"
                      : "self-end border-2 border-emerald-900/20 bg-emerald-900/10 px-1.5 py-1 text-right text-[16px] leading-[1.15]"
                  }
                >
                  {message.text}
                </p>
              ))}
              {pending && (
                <p className="self-start text-[15px] leading-none text-emerald-700">
                  *croaks softly*…
                </p>
              )}
            </div>

            <form onSubmit={send} className="mt-2 flex items-center gap-1.5">
              <input
                ref={inputRef}
                value={text}
                onChange={(event) => setText(event.target.value)}
                maxLength={MAX_INPUT}
                placeholder="what's on your mind"
                aria-label="Message the frog"
                className="min-w-0 flex-1 border-2 border-emerald-900 bg-emerald-50/80 px-1.5 py-0.5 font-[inherit] text-[17px] leading-none text-emerald-950 outline-none placeholder:text-emerald-900/40"
              />
              <button
                type="submit"
                disabled={pending || text.trim().length === 0}
                className="pixel-button px-2 py-0.5 text-[14px] uppercase leading-none text-emerald-900 disabled:opacity-40"
              >
                Send
              </button>
            </form>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {open && (
          <motion.div
            key="status"
            data-interactive
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            transition={{ duration: 0.2 }}
            className="pixel-chip pointer-events-auto relative flex items-center gap-1.5 px-2.5 py-1 text-[15px] uppercase leading-none tracking-[0.06em] text-emerald-50"
          >
            <motion.span
              initial={{ scale: 0.6, opacity: 0.4 }}
              animate={{ scale: [0.8, 1.15, 0.8], opacity: 1 }}
              transition={{ duration: 2, repeat: Infinity, ease: "easeInOut" }}
              className="h-2 w-2 bg-emerald-300"
            />
            here with you
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
