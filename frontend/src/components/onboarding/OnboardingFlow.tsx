import React, { useEffect } from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import {
  WelcomeStep,
  PermissionsStep,
  SetupOverviewStep,
} from './steps';

interface OnboardingFlowProps {
  onComplete: () => void;
}

export function OnboardingFlow({ onComplete }: OnboardingFlowProps) {
  const { currentStep } = useOnboarding();
  // null until the platform check resolves, so step 3 never briefly falls back on macOS
  const [isMac, setIsMac] = React.useState<boolean | null>(null);

  useEffect(() => {
    // Check if running on macOS
    const checkPlatform = async () => {
      try {
        // Dynamic import to avoid SSR issues if any
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        console.error('Failed to detect platform:', e);
        // Fallback
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  // 3-Step Onboarding Flow:
  // Step 1: Welcome - Introduce Morgan Meet features
  // Step 2: Setup Overview - Database initialization
  // Step 3: Permissions - Request mic + system audio (macOS only)

  // A persisted step 3 on non-macOS (old install, or platform detected late) would
  // render nothing, so fall back to the setup overview that finishes onboarding.
  const showSetupOverview = currentStep === 2 || (currentStep === 3 && isMac === false);

  return (
    <div className="onboarding-flow">
      {currentStep === 1 && <WelcomeStep />}
      {showSetupOverview && <SetupOverviewStep />}
      {currentStep === 3 && isMac && <PermissionsStep />}
    </div>
  );
}
