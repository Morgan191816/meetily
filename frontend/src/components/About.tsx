import React, { useState, useEffect } from "react";
import { getVersion } from '@tauri-apps/api/app';
import Image from 'next/image';


export function About() {
    const [currentVersion, setCurrentVersion] = useState<string>('');

    useEffect(() => {
        // Get current version on mount
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    return (
        <div className="p-4 space-y-4 h-[80vh] overflow-y-auto">
            {/* Compact Header */}
            <div className="text-center">
                <div className="mb-3">
                    <Image
                        src="icon_128x128.png"
                        alt="Morgan Meet 标志"
                        width={64}
                        height={64}
                        className="mx-auto"
                    />
                </div>
                <h1 className="text-xl font-bold text-gray-900">Morgan Meet</h1>
                <span className="text-sm text-gray-500"> v{currentVersion}</span>
                <p className="text-medium text-gray-600 mt-1">
                    本地会议转写与纪要工具
                </p>
                <p className="text-xs text-gray-500 mt-1">
                    语音识别模型：SenseVoice（FunAudioLLM）
                </p>
            </div>

            {/* Features Grid - Compact */}
            <div className="space-y-3">
                <h2 className="text-base font-semibold text-gray-800">Morgan Meet 的不同之处</h2>
                <div className="grid grid-cols-2 gap-2">
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">隐私优先</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">数据与 AI 处理流程都可以留在本地，不上云、不外泄。</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">模型自由选择</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">可以用本地开源模型，也可以接入外部 API，不被单一厂商绑定。</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">成本可控</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">本地运行模型，避免按分钟计费；也可以只为选用的调用付费。</p>
                    </div>
                    <div className="bg-gray-50 rounded p-3 hover:bg-gray-100 transition-colors">
                        <h3 className="font-bold text-sm text-gray-900 mb-1">随处可用</h3>
                        <p className="text-xs text-gray-600 leading-relaxed">Google Meet、Zoom、Teams——线上或线下均可。</p>
                    </div>
                </div>
            </div>

            {/* Coming Soon - Compact */}
            <div className="bg-blue-50 rounded p-3">
                <p className="text-s text-blue-800">
                    <span className="font-bold">即将推出：</span>设备端 AI 助手库——自动跟进事项、跟踪行动项等。
                </p>
            </div>
        </div>

    )
}