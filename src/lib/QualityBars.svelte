<script lang="ts">
  let { bars, latency }: { bars: number; latency: number } = $props();
  // faixa por nível: 1-2 ruim, 3 médio, 4-5 bom; 0 = sem túnel
  const q = $derived(bars === 0 ? "none" : bars <= 2 ? "bad" : bars === 3 ? "mid" : "good");
  const label = $derived(
    bars === 0 ? "sem túnel" : latency >= 0 ? `${latency} ms` : "—",
  );
</script>

<span class="jd-q" data-q={q} title="Qualidade da conexão">
  <span class="jd-q-bars">
    {#each [1, 2, 3, 4, 5] as i}<i class:on={i <= bars}></i>{/each}
  </span>
  {label}
</span>
