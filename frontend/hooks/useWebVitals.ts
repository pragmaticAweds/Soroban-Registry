'use client';

import { useReportWebVitals, type Metric } from 'next/web-vitals';

export function useWebVitals() {
  useReportWebVitals((metric: Metric) => {
    const body = JSON.stringify(metric);
    const url = '/api/analytics/vitals';

    // Use `navigator.sendBeacon()` if available, falling back to `fetch()`.
    if (navigator.sendBeacon) {
      navigator.sendBeacon(url, body);
    } else {
      fetch(url, { body, method: 'POST', keepalive: true });
    }

    console.log('[Web Vitals]', metric);
  });
}
