import * as React from "react"

import { cn } from "@/lib/utils"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-[38px] w-full min-w-0 rounded-lg border border-border bg-surface-1 px-3 py-1 text-sm transition-[color,box-shadow] outline-none selection:bg-mint-subtle selection:text-foreground placeholder:text-text-tertiary disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:border-mint-dark focus-visible:shadow-[0_0_0_3px_var(--mint-subtle)]",
        "aria-invalid:border-record-red aria-invalid:shadow-[0_0_0_3px_var(--record-red-dim)]",
        className
      )}
      {...props}
    />
  )
}

export { Input }
