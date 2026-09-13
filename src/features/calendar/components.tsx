'use client';

import { useRouter } from 'next/navigation';
import { useCallback, useEffect, useMemo, useRef, useState, useTransition } from 'react';

import { BookingEditor, CalendarView, IcsImportPreview, ItemEditor, TaskList, TimeZoneReporter, type CalendarOccurrence, type CalendarTaskView, type ItemEditorValue, type TimeZoneReport } from '@isoastra/calendar-react';
import type { ImportPreview } from '@isoastra/calendar-ical';
import type { Booking, CalendarResource, StoredItem } from '@isoastra/fleet-calendar';
import { resolveLocal } from '@isoastra/calendar-core';

import { useRefreshOn } from '@/features/realtime/use-events';
import { userPath, userStreamPath } from '@/lib/paths';
import { applyCalendarImport, previewCalendarImport } from './ical-actions';
import { changeCalendarBooking, createCalendarHold, createCalendarResource } from './booking-actions';
import { completeCalendarTask, createCalendarItem, editCalendarOccurrence, undoCalendarTask } from './item-actions';
import { activateCalendarTimeZoneSession, reportCalendarTimeZone } from './zone-actions';
import { loadCalendarWindow } from './window-actions';

interface CalendarClientProps {
  user: string;
  activeTimeZone: string;
  timeZoneVersion: number;
  occurrences: CalendarOccurrence[];
  tasks: CalendarTaskView[];
  items: StoredItem[];
  resources: CalendarResource[];
  bookings: Booking[];
  viewingOther: boolean;
}

const mutationId = () => globalThis.crypto.randomUUID();

function localValue(instant: string, timeZone: string): string {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
  }).formatToParts(new Date(instant));
  const value = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${value.year}-${value.month}-${value.day}T${value.hour}:${value.minute}`;
}

export function PersonalCalendar(props: CalendarClientProps) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  const [composer, setComposer] = useState<'event' | 'task' | 'work-block' | null>(null);
  const [taskToSchedule, setTaskToSchedule] = useState<CalendarTaskView | null>(null);
  const [selected, setSelected] = useState<CalendarOccurrence | null>(null);
  const [editMode, setEditMode] = useState<'single' | 'all' | 'future'>('single');
  const [editStart, setEditStart] = useState('');
  const [editEnd, setEditEnd] = useState('');
  const [notice, setNotice] = useState('');
  const [source, setSource] = useState('');
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [choices, setChoices] = useState<Record<string, 'replace' | 'skip'>>({});
  const [resourceName, setResourceName] = useState('');
  const [resourceCapacity, setResourceCapacity] = useState(1);
  const [occurrences, setOccurrences] = useState(props.occurrences);
  const windowRequest = useRef(0);
  const [displayTimeZone, setDisplayTimeZone] = useState(props.activeTimeZone);
  useEffect(() => setOccurrences(props.occurrences), [props.occurrences]);
  useEffect(() => setDisplayTimeZone(props.activeTimeZone), [props.activeTimeZone]);
  const changeDisplayTimeZone = useCallback((timeZone: string) => setDisplayTimeZone(timeZone), []);
  const activateZone = useCallback(async (sessionId: string) => {
    const result = await activateCalendarTimeZoneSession(props.user, { sessionId, expectedVersion: props.timeZoneVersion, idempotencyKey: mutationId() });
    if (!result.ok) throw new Error(result.error);
    return result;
  }, [props.timeZoneVersion, props.user]);
  const reportZone = useCallback(async (report: TimeZoneReport) => {
    const result = await reportCalendarTimeZone(props.user, { ...report, idempotencyKey: mutationId() });
    if (!result.ok) throw new Error(result.error);
    if (result.accepted && result.version !== report.expectedVersion) router.refresh();
    return result;
  }, [props.user, router]);
  const changeWindow = useCallback((window: { start: string; end: string; timeZone: string }) => {
    const request = ++windowRequest.current;
    void loadCalendarWindow(props.user, { from: window.start, to: window.end, timeZone: window.timeZone }).then((result) => {
      if (request !== windowRequest.current) return;
      if (result.ok) setOccurrences(result.occurrences);
      else setNotice(result.error);
    });
  }, [props.user]);
  const versions = useMemo(() => new Map(props.items.map((item) => [item.id, item.version])), [props.items]);
  useRefreshOn(userStreamPath(props.user), ['calendar-item', 'calendar-task', 'calendar-scope', 'calendar-booking']);

  function done(result: { ok: boolean; error?: string }, success: string): void {
    setNotice(result.ok ? success : result.error ?? 'The change was not saved.');
    if (result.ok) router.refresh();
  }

  function saveItem(value: ItemEditorValue): void {
    const kind = taskToSchedule ? 'work-block' : composer ?? 'event';
    startTransition(async () => {
      const result = await createCalendarItem(props.user, {
        idempotencyKey: mutationId(),
        kind,
        title: taskToSchedule ? taskToSchedule.title : value.title,
        localStart: 'localStart' in value.timing ? value.timing.localStart : undefined,
        durationMinutes: 'durationMinutes' in value.timing ? value.timing.durationMinutes : undefined,
        timeZone: displayTimeZone,
        followMe: value.followMe,
        rrule: value.rrule,
        linkedTaskId: taskToSchedule?.id,
      });
      done(result, kind === 'work-block' ? 'Work block scheduled.' : 'Event saved.');
      if (result.ok) { setComposer(null); setTaskToSchedule(null); }
    });
  }

  function move(occurrence: CalendarOccurrence, start: string, end: string, mode = editMode): Promise<boolean> {
    return new Promise((resolve) => startTransition(async () => {
      const result = await editCalendarOccurrence(props.user, {
        id: occurrence.seriesId,
        recurrenceId: occurrence.recurrenceId,
        start,
        end,
        mode,
        version: versions.get(occurrence.seriesId) ?? 0,
        idempotencyKey: mutationId(),
      });
      done(result, 'Occurrence moved.');
      resolve(result.ok);
      if (result.ok) setSelected(null);
    }));
  }

  return <div className="personal-calendar" data-pending={pending}>
    <header className="calendar-toolbar">
      <div><TimeZoneReporter
        enabled={!props.viewingOther}
        activeTimeZone={props.activeTimeZone}
        onDisplayTimeZoneChange={changeDisplayTimeZone}
        onActivate={activateZone}
        onReport={reportZone}
      /><small>{props.viewingOther ? 'Viewing this account as an operator. Device zone updates are disabled.' : 'Foreground devices may update this scheduling zone.'}</small></div>
      <div><button type="button" onClick={() => setComposer('event')}>New event</button><button type="button" onClick={() => setComposer('task')}>New task</button><a className="button" href={userPath(props.user, 'calendar/export')}>Download .ics</a></div>
    </header>
    {notice ? <output className="calendar-notice" aria-live="polite">{notice}</output> : null}
    <div className="calendar-layout">
      <section className="calendar-board" aria-label="Calendar">
        <CalendarView
          occurrences={occurrences}
          timeZone={displayTimeZone}
          onWindowChange={changeWindow}
          allowOverlap
          height={720}
          onSelect={(occurrence) => {
            setSelected(occurrence);
            setEditStart(localValue(occurrence.start, displayTimeZone));
            setEditEnd(localValue(occurrence.end, displayTimeZone));
          }}
          onMove={({ id, start, end }) => move(occurrences.find((occurrence) => occurrence.id === id)!, start, end, 'single')}
          renderEvent={({ occurrence, timeText }) => <span className="calendar-event"><strong>{occurrence.title}</strong><small>{timeText}{occurrence.kind === 'work-block' ? ' · task' : ''}</small></span>}
        />
      </section>
      <aside className="calendar-sidebar">
        <h2>Tasks</h2>
        <TaskList
          tasks={props.tasks}
          onComplete={(task) => startTransition(async () => {
            const command = task.completed ? undoCalendarTask : completeCalendarTask;
            done(await command(props.user, { id: task.id, occurrenceId: task.occurrenceId, version: versions.get(task.id) ?? 0, idempotencyKey: mutationId() }), task.completed ? 'Task reopened.' : 'Task completed.');
          })}
          onSchedule={(task) => setTaskToSchedule(task)}
        />
        <details open>
          <summary>Resource booking</summary>
          <form className="calendar-resource-form" onSubmit={(event) => {
            event.preventDefault();
            startTransition(async () => {
              const result = await createCalendarResource(props.user, { name: resourceName, capacity: resourceCapacity, idempotencyKey: mutationId() });
              done(result, 'Resource created.');
              if (result.ok) setResourceName('');
            });
          }}>
            <label>Name<input value={resourceName} onChange={(event) => setResourceName(event.target.value)} required /></label>
            <label>Capacity<input type="number" min={1} value={resourceCapacity} onChange={(event) => setResourceCapacity(Number(event.target.value))} required /></label>
            <button type="submit">Add resource</button>
          </form>
          {props.resources.length ? <BookingEditor resources={props.resources} timeZone={displayTimeZone} onSubmit={(booking) => startTransition(async () => {
            done(await createCalendarHold(props.user, { ...booking, idempotencyKey: mutationId() }), 'Five-minute hold placed.');
          })} /> : <p>Add a resource before booking time.</p>}
          <section aria-label="Bookings" className="calendar-bookings">{props.bookings.map((booking) => <article key={booking.id}>
            <strong>{booking.state}</strong>
            <small>{new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short', timeZone: displayTimeZone }).format(new Date(booking.start))}</small>
            <small>{booking.resources.map(({ resourceId, quantity }) => `${props.resources.find(({ id }) => id === resourceId)?.name ?? resourceId} × ${quantity}`).join(', ')}</small>
            {booking.state === 'hold' ? <button type="button" onClick={() => startTransition(async () => done(await changeCalendarBooking(props.user, { id: booking.id, action: 'confirm', version: booking.version, idempotencyKey: mutationId() }), 'Booking confirmed.'))}>Confirm</button> : null}
            {booking.state !== 'cancelled' ? <button type="button" onClick={() => startTransition(async () => done(await changeCalendarBooking(props.user, { id: booking.id, action: 'cancel', version: booking.version, idempotencyKey: mutationId() }), 'Booking cancelled.'))}>Cancel booking</button> : null}
          </article>)}</section>
        </details>
        <details>
          <summary>Import .ics</summary>
          <label className="file-field">Calendar file<input type="file" accept=".ics,text/calendar" onChange={async (event) => {
            const file = event.target.files?.[0];
            if (!file) return;
            const text = await file.text();
            setSource(text);
            const result = await previewCalendarImport(props.user, text);
            if (result.ok) setPreview(result.preview); else setNotice(result.error);
          }} /></label>
          {preview ? <IcsImportPreview
            summary={preview.entries.map((entry) => ({ key: entry.record.key, title: entry.record.title, action: entry.action, diagnostics: entry.diagnostics.map(({ message }) => message) }))}
            onChoose={(key, choice) => setChoices((current) => ({ ...current, [key]: choice }))}
            onApply={() => startTransition(async () => {
              const result = await applyCalendarImport(props.user, source, choices, mutationId());
              done(result, `${result.applied ?? 0} calendar items imported.`);
              if (result.ok) setPreview(null);
            })}
          /> : null}
        </details>
      </aside>
    </div>
    {composer === 'event' || composer === 'work-block' || taskToSchedule ? <dialog open aria-labelledby="calendar-item-heading" className="calendar-dialog"><h2 id="calendar-item-heading">{taskToSchedule ? 'Schedule task' : 'New event'}</h2><ItemEditor {...(taskToSchedule ? { value: { title: taskToSchedule.title } } : {})} timeZone={displayTimeZone} onSave={saveItem} onCancel={() => { setComposer(null); setTaskToSchedule(null); }} /></dialog> : null}
    {composer === 'task' ? <dialog open aria-labelledby="calendar-task-heading" className="calendar-dialog"><h2 id="calendar-task-heading">New task</h2><TaskComposer timeZone={displayTimeZone} onCancel={() => setComposer(null)} onSave={(input) => startTransition(async () => {
      const result = await createCalendarItem(props.user, { ...input, kind: 'task', idempotencyKey: mutationId(), timeZone: displayTimeZone });
      done(result, 'Task saved.');
      if (result.ok) setComposer(null);
    })} /></dialog> : null}
    {selected ? <dialog open aria-labelledby="calendar-edit-heading" className="calendar-dialog"><h2 id="calendar-edit-heading">Edit occurrence</h2><label>Change<select value={editMode} onChange={(event) => setEditMode(event.target.value as typeof editMode)}><option value="single">This occurrence</option><option value="future">This and future</option><option value="all">Entire series</option></select></label><label>Starts<input type="datetime-local" value={editStart} onChange={(event) => setEditStart(event.target.value)} /></label><label>Ends<input type="datetime-local" value={editEnd} onChange={(event) => setEditEnd(event.target.value)} /></label><div><button type="button" onClick={() => void move(selected, resolveLocal(editStart, displayTimeZone, 'earlier'), resolveLocal(editEnd, displayTimeZone, 'earlier'))}>Save</button><button type="button" onClick={() => setSelected(null)}>Cancel</button></div></dialog> : null}
  </div>;
}

function TaskComposer({ timeZone, onCancel, onSave }: { timeZone: string; onCancel: () => void; onSave: (input: { title: string; due?: string; localStart?: string; durationMinutes?: number; estimateMinutes?: number; followMe: boolean; rrule?: string }) => void }) {
  const [title, setTitle] = useState('');
  const [due, setDue] = useState('');
  const [estimate, setEstimate] = useState(30);
  const [rrule, setRrule] = useState('');
  return <form className="iso-calendar-editor" onSubmit={(event) => { event.preventDefault(); onSave({ title, ...(due ? { due, localStart: due, durationMinutes: estimate } : {}), estimateMinutes: estimate, followMe: false, ...(rrule ? { rrule } : {}) }); }}>
    <label>Title<input required value={title} onChange={(event) => setTitle(event.target.value)} /></label>
    <label>Due<input type="datetime-local" value={due} onChange={(event) => setDue(event.target.value)} /></label>
    <label>Estimate in minutes<input type="number" min={1} value={estimate} onChange={(event) => setEstimate(Number(event.target.value))} /></label>
    <label>Recurrence<input placeholder="FREQ=WEEKLY;COUNT=8" value={rrule} onChange={(event) => setRrule(event.target.value)} /></label>
    <output>Due times use {timeZone}</output>
    <div><button type="submit">Save</button><button type="button" onClick={onCancel}>Cancel</button></div>
  </form>;
}
