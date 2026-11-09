import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

/**
 * Compose Tailwind class names, resolving conditional values via clsx and letting later utilities override earlier conflicting ones via tailwind-merge.
 * @param inputs - Class values (strings, arrays, objects of boolean flags).
 * @returns The merged class string.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
