import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Slot } from "radix-ui"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap text-sm font-medium transition-all outline-none focus-visible:ring-[3px] focus-visible:ring-mint-subtle disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default:
          "bg-mint text-text-inverse rounded-lg shadow-[inset_0_1px_0_rgba(255,255,255,0.15)] hover:brightness-110 hover:shadow-[var(--shadow-mint)]",
        secondary:
          "bg-surface-2 text-foreground border border-border rounded-lg hover:bg-surface-3",
        ghost:
          "text-text-secondary rounded-lg hover:bg-surface-1 hover:text-foreground",
        destructive:
          "bg-record-red-dim text-record-red rounded-lg hover:bg-record-red/20 focus-visible:ring-record-red/20",
        record:
          "bg-record-red text-white rounded-lg animate-record-pulse hover:bg-record-red/90",
        outline:
          "bg-surface-2 text-foreground border border-border rounded-lg hover:bg-surface-3",
        link: "text-mint underline-offset-4 hover:underline",
      },
      size: {
        default: "h-[34px] px-4 py-2 has-[>svg]:px-3",
        xs: "h-6 gap-1 rounded-md px-2 text-xs has-[>svg]:px-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1.5 rounded-md px-3 has-[>svg]:px-2.5",
        lg: "h-[42px] rounded-[10px] px-6 has-[>svg]:px-4",
        icon: "size-[34px]",
        "icon-xs": "size-6 rounded-md [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-7",
        "icon-lg": "size-[42px]",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
