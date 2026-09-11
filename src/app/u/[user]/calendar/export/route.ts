import { NextResponse } from 'next/server';

import { exportPersonalCalendar } from '@/features/calendar/ical-actions';

export async function GET(_request: Request, { params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const calendar = await exportPersonalCalendar(user);
  return new NextResponse(calendar, {
    headers: {
      'cache-control': 'private, no-store',
      'content-disposition': 'attachment; filename="calendar.ics"',
      'content-type': 'text/calendar; charset=utf-8',
    },
  });
}
