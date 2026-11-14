# Project Scope: The Feeling Frog 🐸

## 1. Overview & Mission
**The Feeling Frog** is a cozy, empathetic desktop companion app built for hackathons (targeting **Most Impactful AI** and **Best UI/UX**). Inspired by the desktop-pet mechanics of *Desktop Goose*, The Feeling Frog lives on a floating lily pad in the bottom-right corner of your screen. Instead of causing chaos, it detects invisible cognitive friction, executive dysfunction, ADHD paralysis, and burnout in real time—hopping across your screen to guide you through gentle belly breathing and task-slicing.

---

## 2. The 3 Core Features

1. **Implicit Cognitive Telemetry (Privacy-First)**
   - Background Rust daemon monitoring user workflow rhythms without logging any text content or sensitive personal data.
   - Tracks **Backspace loops** (perfectionism/frustration spirals), **erratic typing cadence**, **mouse jitter**, **tab-thrashing** (rapidly switching window titles), and **idle freezes** over active productivity apps.

2. **Context-Aware Micro-Interventions & AI Frog Brain**
   - When telemetry crosses a stress threshold, the background engine queries a fast LLM API with the active window title and trigger type.
   - Generates a warm, gentle, amphibian-themed micro-intervention message (e.g., "*ribbit* You've hit backspace 40 times in 10 seconds. Let's pause on the lily pad.").

3. **Kinetic Desktop Overlay & Character Physics (UI/UX Masterpiece)**
   - Built with **Tauri v2** as a transparent, frameless, always-on-top window with smart click-through mouse hitboxes.
   - Features spring-physics parabolic hops, vocal-sac throat puffs, interactive glassmorphic speech bubbles, and a **Web Audio API** water-chime breathing modal.

---

## 3. Tech Stack
- **Desktop Shell / Wrapper:** Tauri v2 (Rust backend + React / Vite frontend)
- **Styling & Motion:** Tailwind CSS, Framer Motion (spring physics, layout transitions)
- **Telemetry & OS Hooks (Rust):** `rdev` (global input cadence), `active-win-pos-rs` (active window/tab monitoring), `reqwest` (LLM API calls)
- **Audio:** Native browser Web Audio API (zero external audio asset files required)

---

## 4. Two-Developer Parallel Blueprint

To prevent merge conflicts while vibecoding in parallel, the repository is split into two independent tracks connected via Tauri's IPC event bridge.

### Developer A: Native Shell & Kinetic UI (Frontend Track)
* **Focus:** Tauri window configuration, click-through hitbox management, React frog character rig, Framer Motion spring physics, speech bubbles, and the Lily Pad breathing modal.
* **Key Files:** 
  - `src-tauri/tauri.conf.json` & `capabilities/default.json` (Transparent overlay configuration)
  - `src/components/FeelingFrog.tsx` (Character state machine & hop physics)
  - `src/components/LilyPadGrounding.tsx` (Interactive breath guide + Web Audio synth)

### Developer B: System Daemon & AI Engine (Rust / Backend Track)
* **Focus:** Privacy-first input telemetry (`rdev`), window polling (`active-win-pos-rs`), stress score calculation, LLM prompt pipeline, and Tauri event emission.
* **Key Files:**
  - `src-tauri/src/telemetry.rs` (Global event listeners & ring buffers)
  - `src-tauri/src/window_tracker.rs` (Active app and tab thrashing detector)
  - `src-tauri/src/ai_engine.rs` (LLM JSON payload generator & IPC event emitter)

---

## 5. Shared Contract (`types.ts`)
Both tracks must adhere to this shared TypeScript payload contract for Tauri IPC events:

```typescript
export type BurnoutTrigger = 'frantic_typing' | 'backspace_loop' | 'tab_thrashing' | 'idle_freeze';

export interface FrogInterventionPayload {
  id: string;
  trigger: BurnoutTrigger;
  severity: 'low' | 'medium' | 'high';
  message: string;           
  suggestedAction: 'belly_breathe' | 'lily_pad_step' | 'pond_stretch';
  activeApp?: string;        
}