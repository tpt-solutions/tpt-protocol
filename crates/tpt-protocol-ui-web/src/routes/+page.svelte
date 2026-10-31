<script lang="ts">
  import { onMount } from 'svelte';

  type Quality = 'Good' | 'Bad' | 'Uncertain' | 'Stale';

  interface DataPoint {
    source_protocol: string;
    device_id: string;
    point_id: string;
    timestamp: number;
    value: unknown;
    quality: Quality;
    unit?: string;
  }

  let points: DataPoint[] = [];
  let connected = false;
  let daemonUrl = 'ws://127.0.0.1:9080';
  let error: string | null = null;
  let ws: WebSocket | null = null;

  // Resolve the daemon URL from the Tauri command when running inside the app.
  async function resolveDaemonUrl() {
    try {
      // @ts-ignore - optional in browser
      const { invoke } = await import('@tauri-apps/api/core');
      const cfg = await invoke<{ url: string }>('daemon_url');
      daemonUrl = cfg.url;
    } catch {
      // Running in a plain browser; use default loopback URL.
    }
  }

  function connect() {
    error = null;
    ws = new WebSocket(daemonUrl);
    ws.onopen = () => (connected = true);
    ws.onclose = () => {
      connected = false;
      setTimeout(connect, 3000);
    };
    ws.onerror = () => (error = 'WebSocket error');
    ws.onmessage = (ev) => {
      try {
        const parsed = JSON.parse(ev.data);
        const items = Array.isArray(parsed) ? parsed : [parsed];
        for (const item of items) {
          points = [...points, item].slice(-500);
        }
      } catch {
        error = 'Failed to parse message';
      }
    };
  }

  onMount(async () => {
    await resolveDaemonUrl();
    connect();
  });

  function fmtTime(ts: number) {
    if (!ts) return '';
    const d = new Date(ts);
    return d.toLocaleTimeString();
  }

  function qualityClass(q: Quality) {
    return {
      Good: 'q-good',
      Bad: 'q-bad',
      Uncertain: 'q-uncertain',
      Stale: 'q-stale'
    }[q] ?? 'q-stale';
  }
</script>

<main>
  <header>
    <h1>TPT Protocol — Fleet Dashboard</h1>
    <span class="status" class:online={connected}>{connected ? 'CONNECTED' : 'DISCONNECTED'}</span>
    {#if error}<span class="err">{error}</span>{/if}
  </header>

  <table>
    <thead>
      <tr>
        <th>Time</th><th>Protocol</th><th>Device</th><th>Point</th><th>Value</th><th>Quality</th>
      </tr>
    </thead>
    <tbody>
      {#each points.slice().reverse() as p}
        <tr>
          <td>{fmtTime(p.timestamp)}</td>
          <td>{p.source_protocol}</td>
          <td>{p.device_id}</td>
          <td>{p.point_id}</td>
          <td>{String(p.value)}{p.unit ? ' ' + p.unit : ''}</td>
          <td class={qualityClass(p.quality)}>{p.quality}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #0d1117;
    color: #e6edf3;
    font-family: system-ui, sans-serif;
  }
  main { padding: 1rem; }
  header { display: flex; align-items: center; gap: 1rem; margin-bottom: 1rem; }
  h1 { font-size: 1.2rem; margin: 0; }
  .status { padding: 0.2rem 0.6rem; border-radius: 4px; background: #6e2222; color: #fff; }
  .status.online { background: #1f6f3a; }
  .err { color: #ff7b72; }
  table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
  th, td { text-align: left; padding: 0.35rem 0.6rem; border-bottom: 1px solid #21262d; }
  th { background: #161b22; position: sticky; top: 0; }
  .q-good { color: #3fb950; }
  .q-bad { color: #f85149; }
  .q-uncertain { color: #d29922; }
  .q-stale { color: #8b949e; }
</style>
