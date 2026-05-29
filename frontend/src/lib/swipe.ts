/** A minimal horizontal-swipe touch hook for month navigation. */

import { useRef } from 'react';
import type { TouchEvent } from 'react';

interface SwipeHandlers {
  onTouchStart: (event: TouchEvent) => void;
  onTouchEnd: (event: TouchEvent) => void;
}

interface SwipeOptions {
  onSwipeLeft: () => void;
  onSwipeRight: () => void;
  /** Minimum horizontal distance (px) to count as a swipe. */
  threshold?: number;
}

/**
 * Returns touch handlers that fire `onSwipeLeft` / `onSwipeRight` on a mostly
 * horizontal swipe past `threshold`. Mostly-vertical gestures are ignored so
 * list scrolling is not hijacked.
 */
export function useSwipe({ onSwipeLeft, onSwipeRight, threshold = 50 }: SwipeOptions): SwipeHandlers {
  const start = useRef<{ x: number; y: number } | null>(null);

  return {
    onTouchStart: (event) => {
      const touch = event.changedTouches[0];
      start.current = { x: touch.clientX, y: touch.clientY };
    },
    onTouchEnd: (event) => {
      const origin = start.current;
      if (!origin) {
        return;
      }
      start.current = null;
      const touch = event.changedTouches[0];
      const dx = touch.clientX - origin.x;
      const dy = touch.clientY - origin.y;
      if (Math.abs(dx) < threshold || Math.abs(dx) <= Math.abs(dy)) {
        return; // too small, or mostly vertical
      }
      if (dx < 0) {
        onSwipeLeft();
      } else {
        onSwipeRight();
      }
    },
  };
}
