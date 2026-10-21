<script lang="ts">
  import CircularLoadingIndicator from "./CircularLoadingIndicator.svelte";

  export let label: string;
  export let testId = "pdf-export-progress";
</script>

<div class="pdf-export-progress-overlay" data-testid={testId}>
  <CircularLoadingIndicator
    variant="overlay"
    {label}
    testId={`${testId}-indicator`}
  />
</div>

<style>
  .pdf-export-progress-overlay {
    position: fixed;
    inset: 0;
    z-index: 9999;
    pointer-events: none;
  }
</style>
