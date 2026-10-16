<script lang="ts">
  import { toasts, dismiss } from "./toast.svelte";
</script>

<div class="jd-toasts stack" aria-live="assertive">
  {#each toasts as t (t.id)}
    <div class="jd-toast jd-toast--{t.kind}" role={t.kind === "error" ? "alert" : "status"}>
      <span class="jd-toast-ico">{t.kind === "error" ? "!" : "i"}</span>
      <p>{t.text}</p>
      {#if t.kind === "error"}
        <button class="jd-toast-x" aria-label="Fechar" onclick={() => dismiss(t.id)}>✕</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .stack { position: fixed; top: calc(var(--topbar-h) + var(--space-2)); right: var(--space-4);
    z-index: 100; max-width: calc(100vw - var(--space-4) * 2); }
  .jd-toast { animation: in 0.18s ease-out; }
  .jd-toast p { white-space: pre-wrap; word-break: break-word; }
  @keyframes in { from { opacity: 0; transform: translateY(-6px); } }
  @media (prefers-reduced-motion: reduce) { .jd-toast { animation: none; } }
</style>
