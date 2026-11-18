import React, { useEffect, useState } from 'react';
import { toast } from 'sonner';
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
      toast.error('完成初始化失败，请重试。');
      setIsFinishing(false);
    }
  };

  return (
    <OnboardingContainer
      title="初始化概览"
      description="Morgan Meet 正在准备你的会议库。Whisper 模型可在「设置 → 转写」中下载，纪要模型也在那里配置。"
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
            开始使用
          </Button>
        </div>
      </div>
    </OnboardingContainer>
  );
}
