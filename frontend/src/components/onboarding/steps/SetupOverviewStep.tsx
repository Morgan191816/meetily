import React, { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';

export function SetupOverviewStep() {
  const { goNext, completeOnboarding } = useOnboarding();
  const [isMac, setIsMac] = useState(false);
  const [isFinishing, setIsFinishing] = useState(false);

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  const handleContinue = async () => {
    if (isMac) {
      goNext();
      return;
    }
    setIsFinishing(true);
    try {
      await completeOnboarding();
      window.location.reload();
    } catch (error) {
      console.error('Failed to complete onboarding:', error);
      setIsFinishing(false);
    }
  };

  return (
    <OnboardingContainer
      title="Setup Overview"
      description="Morgan Meet is preparing your meeting library. Transcription and summarization models can be configured later in Settings."
      step={2}
      totalSteps={isMac ? 3 : 2}
    >
      <div className="flex flex-col items-center space-y-10">
        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-4">
          <Button
            onClick={handleContinue}
            disabled={isFinishing}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white"
          >
            Let's Go
          </Button>
        </div>
      </div>
    </OnboardingContainer>
  );
}
