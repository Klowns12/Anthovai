import { NextResponse } from 'next/server'
import { Resend } from 'resend'

const resend = new Resend(process.env.RESEND_API_KEY)

export async function POST(request: Request) {
  try {
    const body = await request.json()
    console.log('Form submission received:', body)

    const { data, error } = await resend.emails.send({
      from: 'Anthovai <onboarding@resend.dev>', // Keep onboarding@resend.dev until domain is verified
      to: 'kaiserklowns@gmail.com', // Must be kaiserklowns@gmail.com until domain is verified
      subject: `[Contact Form] ${body.subject} from ${body.name}`,
      text: `Name: ${body.name}\nCompany: ${body.company || 'N/A'}\nEmail: ${body.email}\n\nMessage:\n${body.message}`,
      replyTo: body.email,
    })

    if (error) {
      console.error('Resend error:', error)
      return NextResponse.json({ success: false, error: error.message }, { status: 400 })
    }

    return NextResponse.json({ success: true, message: 'Message sent successfully.' })
  } catch (error) {
    console.error('Contact api error:', error)
    return NextResponse.json({ success: false, error: 'Internal server error.' }, { status: 500 })
  }
}

