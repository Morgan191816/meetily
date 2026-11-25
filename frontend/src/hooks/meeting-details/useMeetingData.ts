import { useState, useEffect } from 'react';
import { MeetingSummary } from '@/types';

interface UseMeetingDataProps {
  meeting: any;
  summaryData: MeetingSummary | null;
  onMeetingUpdated?: () => Promise<void>;
}

export function useMeetingData({ meeting, summaryData }: UseMeetingDataProps) {
  // State
  // Use prop directly since summary generation fetches transcripts independently
  const transcripts = meeting.transcripts;
  const [meetingTitle, setMeetingTitle] = useState(meeting.title || '+ 新会议');
  const [aiSummary, setAiSummary] = useState<MeetingSummary | null>(summaryData);

  // Sync aiSummary state when summaryData prop changes (fixes display of fetched summaries)
  useEffect(() => {
    console.log('[useMeetingData] Syncing summary data from prop:', summaryData ? 'present' : 'null');
    setAiSummary(summaryData);
  }, [summaryData]); // Only trigger when parent prop changes, not when aiSummary changes

  return {
    // State
    transcripts,
    meetingTitle,
    aiSummary,

    // Setters
    setMeetingTitle,
    setAiSummary,
  };
}
