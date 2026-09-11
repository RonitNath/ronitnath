'use client';

import { useRouter } from 'next/navigation';
import { useMemo, useState, useTransition } from 'react';

import { CalendarView, IcsImportPreview, ItemEditor, TaskList, TimeZoneReporter, type CalendarOccurrence, type CalendarTaskView, type ItemEditorValue } from '@isoastra/calendar-react';
import type { ImportPreview } from '@isoastra/calendar-ical';
import type { StoredItem } from '@isoastra/fleet-calendar';
import { resolveLocal } from '@isoastra/calendar-core';

import { useRefreshOn } from '@/features/realtime/use-events';
import { userPath, userStreamPath } from '@/lib/paths';
import { applyCalendarImport, previewCalendarImport } from './ical-actions';
import { completeCalendarTask, createCalendarItem, editCalendarOccurrence } from './item-actions';
import { reportCalendarTimeZone } from './zone-actions';

interface CalendarClientProps {
  user: string;
  activeTimeZone: string;
  timeZoneVersion: number;
  occurrences: CalendarOccurrence[];
  tasks: CalendarTaskView[];
  items: StoredItem[];
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
        timeZone: props.activeTimeZone,
        followMe: value.followMe,
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
        onReport={async (report) => {
          const result = await reportCalendarTimeZone(props.user, { ...report, expectedVersion: props.timeZoneVersion, idempotencyKey: mutationId() });
          if (result.ok && result.accepted) router.refresh();
        }}
      /><small>{props.viewingOther ? 'Viewing this account as an operator. Device zone updates are disabled.' : 'Foreground devices may update this scheduling zone.'}</small></div>
      <div><button type="button" onClick={() => setComposer('event')}>New event</button><button type="button" onClick={() => setComposer('task')}>New task</button><a className="button" href={userPath(props.user, 'calendar/export')}>Download .ics</a></div>
    </header>
    {notice ? <output className="calendar-notice" aria-live="polite">{notice}</output> : null}
    <div className="calendar-layout">
      <section className="calendar-board" aria-label="Calendar">
        <CalendarView
          occurrences={props.occurrences}
          timeZone={props.activeTimeZone}
          allowOverlap
          height={720}
          onSelect={(occurrence) => {
            setSelected(occurrence);
            setEditStart(localValue(occurrence.start, props.activeTimeZone));
            setEditEnd(localValue(occurrence.end, props.activeTimeZone));
          }}
          onMove={({ id, start, end }) => move(props.occurrences.find((occurrence) => occurrence.id === id)!, start, end, 'single')}
          renderEvent={({ occurrence, timeText }) => <span className="calendar-event"><strong>{occurrence.title}</strong><small>{timeText}{occurrence.kind === 'work-block' ? ' · task' : ''}</small></span>}
        />
      </section>
      <aside className="calendar-sidebar">
        <h2>Tasks</h2>
        <TaskList
          tasks={props.tasks}
          onComplete={(task) => startTransition(async () => done(await completeCalendarTask(props.user, { id: task.id, occurrenceId: task.occurrenceId, version: versions.get(task.id) ?? 0, idempotencyKey: mutationId() }), 'Task completed.'))}
          onSchedule={(task) => setTaskToSchedule(task)}
        />
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
    {composer === 'event' || composer === 'work-block' || taskToSchedule ? <dialog open aria-labelledby="calendar-item-heading" className="calendar-dialog"><h2 id="calendar-item-heading">{taskToSchedule ? 'Schedule task' : 'New event'}</h2><ItemEditor {...(taskToSchedule ? { value: { title: taskToSchedule.title } } : {})} timeZone={props.activeTimeZone} onSave={saveItem} onCancel={() => { setComposer(null); setTaskToSchedule(null); }} /></dialog> : null}
    {composer === 'task' ? <dialog open aria-labelledby="calendar-task-heading" className="calendar-dialog"><h2 id="calendar-task-heading">New task</h2><TaskComposer timeZone={props.activeTimeZone} onCancel={() => setComposer(null)} onSave={(input) => startTransition(async () => {
      const result = await createCalendarItem(props.user, { ...input, kind: 'task', idempotencyKey: mutationId(), timeZone: props.activeTimeZone });
      done(result, 'Task saved.');
      if (result.ok) setComposer(null);
    })} /></dialog> : null}
    {selected ? <dialog open aria-labelledby="calendar-edit-heading" className="calendar-dialog"><h2 id="calendar-edit-heading">Edit occurrence</h2><label>Change<select value={editMode} onChange={(event) => setEditMode(event.target.value as typeof editMode)}><option value="single">This occurrence</option><option value="future">This and future</option><option value="all">Entire series</option></select></label><label>Starts<input type="datetime-local" value={editStart} onChange={(event) => setEditStart(event.target.value)} /></label><label>Ends<input type="datetime-local" value={editEnd} onChange={(event) => setEditEnd(event.target.value)} /></label><div><button type="button" onClick={() => void move(selected, resolveLocal(editStart, props.activeTimeZone, 'earlier'), resolveLocal(editEnd, props.activeTimeZone, 'earlier'))}>Save</button><button type="button" onClick={() => setSelected(null)}>Cancel</button></div></dialog> : null}
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
