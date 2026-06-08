import { useEffect, useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Search } from 'lucide-react'
import { Input } from '@/shared/components/ui/input'

export function SearchBar() {
  const inputRef = useRef<HTMLInputElement | null>(null)
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const [query, setQuery] = useState(searchParams.get('q') ?? '')

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        inputRef.current?.focus()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const trimmed = query.trim()
    if (trimmed) {
      navigate(`/mail/search?q=${encodeURIComponent(trimmed)}`)
    }
  }

  return (
    <form className="relative w-full max-w-xl" onSubmit={handleSubmit}>
      <Search className="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground" aria-hidden="true" />
      <Input
        ref={inputRef}
        value={query}
        onChange={(event) => setQuery(event.currentTarget.value)}
        className="pl-9"
        placeholder="Search mail"
        aria-label="Search mail"
      />
      <kbd className="pointer-events-none absolute right-3 top-2.5 rounded border border-border px-1.5 text-[10px] text-muted-foreground">
        Ctrl K
      </kbd>
    </form>
  )
}
