'use client';

import React, { ReactNode } from 'react';

interface AnalyticsProviderProps {
  children: ReactNode;
}

// Telemetry is disabled: the provider only renders its children.
export default function AnalyticsProvider({ children }: AnalyticsProviderProps) {
  return <>{children}</>;
}
