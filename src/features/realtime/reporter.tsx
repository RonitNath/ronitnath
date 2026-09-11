'use client';

import { usePathname, useRouter } from 'next/navigation';
import { useEffect, useRef } from 'react';
import { HEARTBEAT_MS, type RealtimeSubscription } from '@isoastra/fleet-events/presence';

const uuid = () => crypto.randomUUID();
function held(store: Storage, key: string): string {
  const found = store.getItem(key);
  if (found) return found;
  const made = uuid();
  store.setItem(key, made);
  return made;
}
function temporaryVisitor(): string {
  const id = localStorage.getItem('rn_realtime_visitor');
  const expiresKey = 'rn_realtime_visitor_expires';
  const expires = Number(localStorage.getItem(expiresKey) ?? 0);
  if (id && expires > Date.now()) return id;
  if (id && expires === 0) {
    localStorage.setItem(expiresKey, String(Date.now() + 7 * 24 * 60 * 60_000));
    return id;
  }
  const made = uuid();
  sessionStorage.removeItem('rn_realtime_tab');
  sessionStorage.removeItem('rn_realtime_view');
  localStorage.setItem('rn_realtime_visitor', made);
  localStorage.setItem(expiresKey, String(Date.now() + 7 * 24 * 60 * 60_000));
  return made;
}

function dependencies(): RealtimeSubscription[] {
  const subscriptions: RealtimeSubscription[] = [];
  document.querySelectorAll<HTMLElement>('[data-realtime-resource]').forEach((node) => {
    const [resourceKind, resourceId] = (node.dataset.realtimeResource ?? '').split(':');
    if (resourceKind && resourceId)
      subscriptions.push({ type: 'resource', resourceKind, resourceId });
  });
  document.querySelectorAll<HTMLElement>('[data-realtime-config]').forEach((node) => {
    const [key, state] = (node.dataset.realtimeConfig ?? '').split(':');
    if (key && (state === 'draft' || state === 'published' || state === 'live'))
      subscriptions.push({ type: 'configuration', key, state });
  });
  return subscriptions;
}

export function RealtimeReporter() {
  const pathname = usePathname();
  const router = useRouter();
  const stream = useRef<EventSource | null>(null);
  useEffect(() => {
    const visitorId = temporaryVisitor();
    const tabId = held(sessionStorage, 'rn_realtime_tab');
    const viewId = held(sessionStorage, 'rn_realtime_view');
    let interactedAt: string | null = null;
    let stopped = false;
    const connect = () => {
      if (stopped || stream.current) return;
      const source = new EventSource(
        `/api/realtime/stream?visitor=${visitorId}&view=${viewId}`,
      );
      stream.current = source;
      source.onmessage = (message) => {
        const delivery = JSON.parse(message.data) as {
          id: string;
          revision: string;
          resourceKind: string;
          resourceId: string;
          kind: string;
        };
        const acknowledge = (stage: 'received' | 'applied') =>
          fetch('/api/realtime', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              action: 'acknowledge',
              visitorId,
              viewId,
              acknowledgment: { deliveryId: delivery.id, stage, revision: delivery.revision },
            }),
          });
        void acknowledge('received');
        window.dispatchEvent(new CustomEvent('realtime:update', { detail: delivery }));
        router.refresh();
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            void acknowledge('applied');
          }),
        );
      };
      source.onerror = () => {
        source.close();
        if (stream.current === source) stream.current = null;
      };
    };
    const report = async (action: 'register' | 'heartbeat') => {
      const sections = [...document.querySelectorAll<HTMLElement>('[data-realtime-section]')]
        .filter(
          (node) =>
            node.getBoundingClientRect().bottom > 0 &&
            node.getBoundingClientRect().top < innerHeight,
        )
        .map((node) => node.dataset.realtimeSection!)
        .slice(0, 100);
      const rows = [...document.querySelectorAll<HTMLElement>('[data-realtime-row]')]
        .filter(
          (node) =>
            node.getBoundingClientRect().bottom > 0 &&
            node.getBoundingClientRect().top < innerHeight,
        )
        .map((node) => node.dataset.realtimeRow!)
        .slice(0, 500);
      const response = await fetch('/api/realtime', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          action,
          view: {
            visitorId,
            tabId,
            viewId,
            path: pathname,
            title: document.title,
            subscriptions: dependencies(),
            viewport: {
              visible: document.visibilityState === 'visible',
              sectionIds: sections,
              rowIds: rows,
            },
            interactedAt,
          },
        }),
      });
      if (response.ok) connect();
    };
    const interact = () => {
      interactedAt = new Date().toISOString();
    };
    let frame = 0;
    const reportViewport = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        void report('heartbeat');
      });
    };
    void report('register');
    const timer = setInterval(() => {
      void report('heartbeat');
    }, HEARTBEAT_MS);
    const observer = new MutationObserver(() => {
      void report('heartbeat');
    });
    observer.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ['data-realtime-resource', 'data-realtime-config'],
    });
    addEventListener('pointerdown', interact, { passive: true });
    addEventListener('keydown', interact);
    addEventListener('scroll', reportViewport, { passive: true });
    addEventListener('resize', reportViewport, { passive: true });
    document.addEventListener('visibilitychange', reportViewport);
    return () => {
      stopped = true;
      clearInterval(timer);
      observer.disconnect();
      stream.current?.close();
      stream.current = null;
      removeEventListener('pointerdown', interact);
      removeEventListener('keydown', interact);
      removeEventListener('scroll', reportViewport);
      removeEventListener('resize', reportViewport);
      document.removeEventListener('visibilitychange', reportViewport);
      cancelAnimationFrame(frame);
      void fetch('/api/realtime', {
        method: 'POST',
        keepalive: true,
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ action: 'disconnect', visitorId, viewId }),
      });
    };
  }, [pathname, router]);
  return null;
}
