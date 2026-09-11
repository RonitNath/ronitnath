import type { Metadata } from 'next';

import '@isoastra/calendar-react/styles.css';

import { PersonalCalendar } from '@/features/calendar/components';
import { loadPersonalCalendar } from '@/features/calendar/queries';

export const metadata: Metadata = { title: 'Calendar' };

export default async function CalendarPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const calendar = await loadPersonalCalendar(user);
  return <main className="indoors calendar-page">
    <h1>Calendar</h1>
    <PersonalCalendar user={user} {...calendar} />
  </main>;
}
