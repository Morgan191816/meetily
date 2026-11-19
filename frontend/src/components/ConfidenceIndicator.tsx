'use client';

interface ConfidenceIndicatorProps {
  confidence?: number;
  estimated?: boolean;
  showIndicator?: boolean;
}

export const ConfidenceIndicator: React.FC<ConfidenceIndicatorProps> = ({
  confidence,
  estimated = false,
  showIndicator = true,
}) => {
  // Don't render if preference is disabled or there is no real score
  if (!showIndicator || confidence === undefined) {
    return null;
  }

  // Get color class based on confidence threshold
  const getColorClass = (conf: number): string => {
    if (conf >= 0.8) return 'bg-green-500'; // 80-100%：置信度高
    if (conf >= 0.7) return 'bg-yellow-500'; // 70-79%：置信度良好
    if (conf >= 0.4) return 'bg-orange-500'; // 40-79%：置信度中等
    return 'bg-red-500'; // 50% 以下：置信度低
  };

  // Get descriptive label for accessibility
  const getConfidenceLabel = (conf: number): string => {
    if (conf >= 0.8) return '置信度高';
    if (conf >= 0.7) return '置信度良好';
    if (conf >= 0.4) return '置信度中等';
    return '置信度低';
  };

  const confidencePercent = (confidence * 100).toFixed(0);
  const colorClass = getColorClass(confidence);
  const title = estimated
    ? `置信度 ${confidencePercent}%（估算）`
    : `置信度 ${confidencePercent}% - ${getConfidenceLabel(confidence)}`;

  return (
    <div
      className="flex items-center gap-1"
      title={title}
      aria-label={`转写置信度：${confidencePercent}%${estimated ? '（估算）' : ''}`}
    >
      <div
        className={`w-2 h-2 rounded-full ${colorClass} transition-colors duration-200`}
        role="status"
      />
    </div>
  );
};
