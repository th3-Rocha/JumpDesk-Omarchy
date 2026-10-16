<script lang="ts">
  // StatusPill do design system: bolinha + nome + estado, sempre com a palavra.
  type Stage = "idle" | "connecting" | "up" | "error";
  let { label, stage }: { label: string; stage: Stage } = $props();

  const s: Record<Stage, string> = { idle: "idle", connecting: "busy", up: "ok", error: "error" };
  const text: Record<Stage, string> = {
    idle: "parado",
    connecting: "conectando…",
    up: "conectado",
    error: "erro",
  };
</script>

<div class="jd-status" data-s={s[stage]}>
  <span class="jd-dot" data-s={s[stage]}></span>
  <b>{label}</b>
  <span>{text[stage]}</span>
</div>
