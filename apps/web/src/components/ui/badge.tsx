import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-full border border-transparent h-[22px] px-2 text-xs font-medium whitespace-nowrap transition-[color,box-shadow] [&>svg]:pointer-events-none [&>svg]:size-3",
  {
    variants: {
      variant: {
        mint: "bg-mint-subtle text-mint border-mint/20",
        red: "bg-record-red-dim text-record-red border-record-red/20",
        warning: "bg-warning-dim text-warning border-warning/20",
        neutral: "bg-surface-2 text-text-secondary border-border",
        outline: "border-border text-foreground",
      },
    },
    defaultVariants: {
      variant: "mint",
    },
  }
)

function Badge({
  className,
  variant = "mint",
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "span"

  return (
    <Comp
      data-slot="badge"
      data-variant={variant}
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  )
}

export { Badge, badgeVariants }
