<script lang="ts">
  // Fontes empacotadas no app: com a VPN de pé o Google Fonts pode não abrir.
  import "@fontsource/ibm-plex-sans/latin-400.css";
  import "@fontsource/ibm-plex-sans/latin-500.css";
  import "@fontsource/ibm-plex-sans/latin-600.css";
  import "@fontsource/ibm-plex-mono/latin-400.css";
  import "@fontsource/ibm-plex-mono/latin-500.css";
  import "$lib/styles/tokens.css";
  import "$lib/styles/jd.css";

  let { children } = $props();
</script>

<div class="jd root">{@render children()}</div>

<style>
  :global(html, body) { margin: 0; background: var(--bg); }
  .root { min-height: 100vh; }
</style>
